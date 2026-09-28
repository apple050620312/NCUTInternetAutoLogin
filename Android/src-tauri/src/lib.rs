#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_ncut_mobile::init())
        .run(tauri::generate_context!())
        .expect("Unable to start NCUT app");
}

#[cfg(target_os = "android")]
mod bridge {
    use jni::{
        objects::{JObject, JString},
        sys::{jboolean, jstring},
        JNIEnv,
    };
    use ncut_core::{Engine, Status};
    use std::sync::OnceLock;

    #[no_mangle]
    pub extern "system" fn Java_tw_edu_ncut_autologin_mobile_NativeBridge_tick(
        mut env: JNIEnv,
        _object: JObject,
        username: JString,
        password: JString,
        login: jboolean,
    ) -> jstring {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let username: String = env.get_string(&username)?.into();
            let password: String = env.get_string(&password)?.into();
            static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
            let runtime =
                RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("Network runtime"));
            let status = runtime.block_on(async {
                let Ok(engine) = Engine::new() else {
                    return Status::Unstable;
                };
                let status = engine.check().await;
                if login != 0 && status == Status::NeedsLogin {
                    engine
                        .connect(&username, &password)
                        .await
                        .unwrap_or(Status::LoginFailed)
                } else {
                    status
                }
            });
            Ok::<_, jni::errors::Error>(
                serde_json::to_value(status)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned(),
            )
        }));
        let status = match result {
            Ok(Ok(value)) => value,
            _ => "unstable".to_owned(),
        };
        env.new_string(status)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }
}
