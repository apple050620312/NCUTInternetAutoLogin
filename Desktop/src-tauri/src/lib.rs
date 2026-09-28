use ncut_core::{Engine, Settings, Status};
use serde::Serialize;
use std::{
    collections::VecDeque,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use tauri_plugin_autostart::ManagerExt;

#[derive(Clone, Serialize)]
struct Activity {
    at: u64,
    status: Status,
    message: String,
}
#[derive(Clone, Serialize)]
struct Snapshot {
    settings: Settings,
    status: Status,
    history: Vec<Activity>,
    has_password: bool,
}
struct Data {
    settings: Settings,
    status: Status,
    history: VecDeque<Activity>,
    has_password: bool,
}
struct AppState {
    data: Mutex<Data>,
    operation: tokio::sync::Mutex<()>,
    generation: AtomicU64,
    wake: tokio::sync::Notify,
    path: PathBuf,
}

fn credential(username: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("tw.edu.ncut.autologin", username).map_err(|_| "無法存取系統憑證庫".into())
}
fn snapshot(state: &AppState) -> Snapshot {
    let data = state.data.lock().unwrap();
    Snapshot {
        settings: data.settings.clone(),
        status: data.status,
        history: data.history.iter().cloned().collect(),
        has_password: data.has_password,
    }
}
fn publish(app: &AppHandle, status: Status, message: &str) {
    let state = app.state::<AppState>();
    {
        let mut data = state.data.lock().unwrap();
        data.status = status;
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if status != Status::Checking
            && data
                .history
                .back()
                .map(|event| event.status != status)
                .unwrap_or(true)
        {
            data.history.push_back(Activity {
                at,
                status,
                message: message.into(),
            });
            if data.history.len() > 100 {
                data.history.pop_front();
            }
        }
    }
    let _ = app.emit("network-status", snapshot(&state));
}
fn status_message(status: Status) -> &'static str {
    match status {
        Status::Online => "網路連線正常",
        Status::NeedsLogin => "偵測到校園登入頁面",
        Status::Unstable => "無法確認網路連線，稍後重試",
        Status::Authenticating => "正在登入校園網路",
        Status::Checking => "正在檢查網路連線",
        Status::Stopped => "自動連線已暫停",
        Status::MissingCredentials => "請設定帳號與密碼",
        Status::LoginFailed => "登入未完成，請確認帳號與網路",
    }
}

#[tauri::command]
fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
    snapshot(&state)
}

#[tauri::command]
async fn save_settings(
    app: AppHandle,
    settings: Settings,
    password: Option<String>,
) -> Result<Snapshot, String> {
    settings
        .validate()
        .map_err(|_| "檢查間隔須介於 5 至 3600 秒".to_string())?;
    if settings.username.trim().is_empty() {
        return Err("請輸入帳號".into());
    }
    let state = app.state::<AppState>();
    let _operation = state.operation.lock().await;
    let old = snapshot(&state);
    let entry = credential(&settings.username)?;
    if let Some(password) = password.as_ref() {
        if password.is_empty() {
            return Err("密碼不可為空白".into());
        }
        entry
            .set_password(password)
            .map_err(|_| "無法儲存密碼至系統憑證庫".to_string())?;
    }
    let has_password = entry.get_password().map(|s| !s.is_empty()).unwrap_or(false);
    if !has_password {
        return Err("請輸入密碼；Linux 請確認 Secret Service 已啟動".into());
    }
    let startup = app.autolaunch();
    if settings.start_at_login {
        startup.enable()
    } else {
        startup.disable()
    }
    .map_err(|_| "無法更新開機啟動設定".to_string())?;
    let bytes = serde_json::to_vec_pretty(&settings).map_err(|_| "設定序列化失敗".to_string())?;
    {
        let mut data = state.data.lock().unwrap();
        if write_settings(&state.path, &bytes).is_err() {
            let _ = if old.settings.start_at_login {
                startup.enable()
            } else {
                startup.disable()
            };
            return Err("無法寫入設定檔".into());
        }
        state.generation.fetch_add(1, Ordering::SeqCst);
        data.settings = settings;
        data.has_password = has_password;
    }
    state.wake.notify_one();
    publish(
        &app,
        if snapshot(&state).settings.auto_connect {
            Status::Checking
        } else {
            Status::Stopped
        },
        "設定已儲存",
    );
    Ok(snapshot(&state))
}

#[tauri::command]
async fn set_monitoring(app: AppHandle, enabled: bool) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    {
        let mut data = state.data.lock().unwrap();
        if enabled && !data.has_password {
            return Err("請先儲存帳號與密碼".into());
        }
        let mut settings = data.settings.clone();
        settings.auto_connect = enabled;
        let bytes =
            serde_json::to_vec_pretty(&settings).map_err(|_| "設定序列化失敗".to_string())?;
        write_settings(&state.path, &bytes).map_err(|_| "無法寫入設定檔".to_string())?;
        data.settings = settings;
        state.generation.fetch_add(1, Ordering::SeqCst);
    }
    state.wake.notify_one();
    let status = if enabled {
        Status::Checking
    } else {
        Status::Stopped
    };
    publish(&app, status, status_message(status));
    Ok(snapshot(&state))
}

fn write_settings(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}

async fn perform(app: &AppHandle, login: bool) -> Snapshot {
    let state = app.state::<AppState>();
    let _operation = state.operation.lock().await;
    let generation = state.generation.load(Ordering::SeqCst);
    let settings = snapshot(&state).settings;
    let engine = match Engine::new() {
        Ok(e) => e,
        Err(_) => {
            publish(app, Status::Unstable, status_message(Status::Unstable));
            return snapshot(&state);
        }
    };
    publish(app, Status::Checking, status_message(Status::Checking));
    let mut status = engine.check().await;
    if generation != state.generation.load(Ordering::SeqCst) {
        return snapshot(&state);
    }
    if login && status == Status::NeedsLogin {
        let password = credential(&settings.username)
            .and_then(|e| e.get_password().map_err(|_| "無法讀取密碼".into()));
        status = match password {
            Ok(password) => {
                publish(
                    app,
                    Status::Authenticating,
                    status_message(Status::Authenticating),
                );
                engine
                    .connect(&settings.username, &password)
                    .await
                    .unwrap_or(Status::LoginFailed)
            }
            Err(_) => Status::MissingCredentials,
        };
    }
    if generation == state.generation.load(Ordering::SeqCst) {
        publish(app, status, status_message(status));
    }
    snapshot(&state)
}
#[tauri::command]
async fn check_now(app: AppHandle) -> Snapshot {
    perform(&app, false).await
}
#[tauri::command]
async fn connect_now(app: AppHandle) -> Snapshot {
    perform(&app, true).await
}
#[tauri::command]
fn clear_history(app: AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    state.data.lock().unwrap().history.clear();
    let result = snapshot(&state);
    let _ = app.emit("network-status", &result);
    result
}

fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if !args.iter().any(|arg| arg == "--background") { show(app); }
        }))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--background"])))
        .invoke_handler(tauri::generate_handler![get_snapshot, save_settings, set_monitoring, check_now, connect_now, clear_history])
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            fs::create_dir_all(&dir)?;
            let path = dir.join("settings.json");
            let mut settings: Settings = fs::read(&path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
            if settings.validate().is_err() { settings = Settings::default(); }
            settings.start_at_login = app.autolaunch().is_enabled().unwrap_or(false);
            let has_password = !settings.username.is_empty() && credential(&settings.username).and_then(|e| e.get_password().map_err(|_| String::new())).map(|s| !s.is_empty()).unwrap_or(false);
            app.manage(AppState { data: Mutex::new(Data { settings, status: Status::Stopped, history: VecDeque::new(), has_password }), operation: tokio::sync::Mutex::new(()), generation: AtomicU64::new(0), wake: tokio::sync::Notify::new(), path });
            let open = MenuItem::with_id(app, "open", "開啟 NCUT 校園網路", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause", "暫停自動連線", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "結束", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &pause, &quit])?;
            let mut tray = TrayIconBuilder::new().menu(&menu).tooltip("NCUT 校園網路")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show(app),
                    "pause" => { let app = app.clone(); tauri::async_runtime::spawn(async move { let _ = set_monitoring(app, false).await; }); },
                    "quit" => app.exit(0),
                    _ => {}
                }).on_tray_icon_event(|tray, event| if matches!(event, TrayIconEvent::DoubleClick { .. }) { show(tray.app_handle()); });
            if let Some(icon) = app.default_window_icon() { tray = tray.icon(icon.clone()); }
            tray.build(app)?;
            if std::env::args().any(|arg| arg == "--background") {
                if let Some(window) = app.get_webview_window("main") { let _ = window.hide(); }
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let state = handle.state::<AppState>();
                    let settings = snapshot(&state).settings;
                    if settings.auto_connect { perform(&handle, true).await; }
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(settings.interval_seconds)) => {},
                        _ = state.wake.notified() => {}
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event { api.prevent_close(); let _ = window.hide(); }
        })
        .run(tauri::generate_context!()).expect("Unable to run NCUT desktop app");
}
