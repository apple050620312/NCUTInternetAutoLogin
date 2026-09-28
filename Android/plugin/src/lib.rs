#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[tauri::command]
async fn execute<R: Runtime>(
    app: tauri::AppHandle<R>,
    action: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    if ![
        "snapshot", "save", "start", "stop", "check", "connect", "forget",
    ]
    .contains(&action.as_str())
    {
        return Err("Unknown action".into());
    }
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<tauri::plugin::PluginHandle<R>>();
        handle
            .run_mobile_plugin(&action, payload)
            .map_err(|error| match error {
                tauri::plugin::mobile::PluginInvokeError::InvokeRejected(response) => response
                    .message
                    .unwrap_or_else(|| "操作未完成，請稍後重試".into()),
                _ => "操作未完成，請稍後重試".into(),
            })
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, payload);
        Err("此頁面尚未連接手機程式".into())
    }
}
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("ncut-mobile")
        .invoke_handler(tauri::generate_handler![execute])
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    _api.register_android_plugin("tw.edu.ncut.autologin.mobile", "NcutPlugin")?;
                _app.manage(handle);
            }
            Ok(())
        })
        .build()
}
