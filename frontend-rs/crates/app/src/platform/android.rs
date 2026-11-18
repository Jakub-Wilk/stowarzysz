//! Android: the Rust side runs natively (the UI is drawn in a system WebView). Java APIs are
//! reached through JNI and `Bridge` in `android/MainActivity.kt`; this is the only module that
//! needs `unsafe` (handing ndk-context's raw pointers to the `jni` crate).

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Duration;

use api::{LockGuard, TokenStore};
use futures_util::future::LocalBoxFuture;
use jni::objects::{JClass, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

use super::Kind;

pub const KIND: Kind = Kind::Android;

pub fn api_origin() -> String {
    crate::config::ANDROID_ORIGIN.to_owned()
}

pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

pub fn init() {
    let _ = files_dir();
}

/// Run `f` with the JNI environment and the activity.
fn with_activity<R>(
    f: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> jni::errors::Result<R>,
) -> Result<R, String> {
    let context = ndk_context::android_context();
    // SAFETY: ndk-context holds the process's JavaVM, valid for the life of the process.
    #[allow(unsafe_code)]
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    // SAFETY: a global reference to the activity owned by the glue code; `JObject` never
    // deletes the reference it wraps.
    #[allow(unsafe_code)]
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    let result = f(&mut env, &activity);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result.map_err(|e| e.to_string())
}

/// `dev.dioxus.main.Bridge`, through the app's class loader (a native thread's default loader
/// only sees system classes).
fn bridge<'a>(env: &mut JNIEnv<'a>, activity: &JObject<'_>) -> jni::errors::Result<JClass<'a>> {
    let loader = env
        .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
        .l()?;
    let name = env.new_string("dev.dioxus.main.Bridge")?;
    let class = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&name)],
        )?
        .l()?;
    Ok(JClass::from(class))
}

fn optional_string(
    env: &mut JNIEnv<'_>,
    object: JObject<'_>,
) -> jni::errors::Result<Option<String>> {
    if object.is_null() {
        return Ok(None);
    }
    Ok(Some(env.get_string(&JString::from(object))?.into()))
}

/// `Context.getFilesDir()`: private to the app, kept across updates, removed on uninstall.
fn files_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        with_activity(|env, activity| {
            let dir = env
                .call_method(activity, "getFilesDir", "()Ljava/io/File;", &[])?
                .l()?;
            let path = env
                .call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])?
                .l()?;
            Ok(PathBuf::from(
                optional_string(env, path)?.unwrap_or_default(),
            ))
        })
        .unwrap_or_default()
    })
}

pub fn set_system_bar_color(r: u8, g: u8, b: u8) {
    let argb = i32::from_be_bytes([0xff, r, g, b]);
    // Dark icons on a light background.
    let luminance = 0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b);
    let light = luminance > 150.0;
    let _ = with_activity(|env, activity| {
        let class = bridge(env, activity)?;
        env.call_static_method(
            &class,
            "setSystemBars",
            "(Landroid/app/Activity;IZ)V",
            &[
                JValue::Object(activity),
                JValue::Int(argb),
                JValue::Bool(light.into()),
            ],
        )?;
        Ok(())
    });
}

/// Settings in one JSON object in `files/settings.json`, cached in memory.
pub mod storage {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock, PoisonError};

    fn path() -> std::path::PathBuf {
        super::files_dir().join("settings.json")
    }

    fn values() -> &'static Mutex<HashMap<String, String>> {
        static VALUES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
        VALUES.get_or_init(|| {
            let stored = std::fs::read(path())
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            Mutex::new(stored)
        })
    }

    pub fn get(key: &str) -> Option<String> {
        values()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    pub fn set(key: &str, value: Option<&str>) {
        let mut values = values().lock().unwrap_or_else(PoisonError::into_inner);
        let changed = match value {
            Some(value) => {
                values.insert(key.to_owned(), value.to_owned()).as_deref() != Some(value)
            }
            None => values.remove(key).is_some(),
        };
        if changed {
            // Write a new file and rename it over the old one, so a crash never leaves half.
            let path = path();
            let temp = path.with_extension("json.tmp");
            if let Ok(json) = serde_json::to_vec(&*values) {
                if std::fs::write(&temp, json).is_ok() {
                    let _ = std::fs::rename(&temp, &path);
                }
            }
        }
    }
}

struct AndroidTokens;

impl TokenStore for AndroidTokens {
    fn load(&self) -> Option<String> {
        storage::get("auth.refresh")
    }

    fn save(&self, refresh: Option<&str>) {
        storage::set("auth.refresh", refresh);
    }

    fn lock(&self) -> LocalBoxFuture<'_, LockGuard> {
        // One process, and the client already refreshes single-flight.
        Box::pin(async { LockGuard::none() })
    }
}

pub fn token_store() -> Rc<dyn TokenStore> {
    Rc::new(AndroidTokens)
}

/// FCM arrives with phase 6 of the plan (backend sender + `PushService`); until then the
/// toggle stays hidden.
pub mod push {
    use api::Client;

    use crate::platform::PushStatus;

    pub async fn status(_client: &Client) -> PushStatus {
        PushStatus::Unconfigured
    }

    pub async fn enable(_client: &Client) -> Result<(), String> {
        Err("Powiadomienia w aplikacji na Androida jeszcze nie działają.".to_owned())
    }

    pub async fn disable(_client: &Client) -> Result<(), String> {
        Ok(())
    }
}

pub mod updater {
    use std::path::PathBuf;

    use futures_util::StreamExt as _;
    use jni::objects::JValue;
    use sha2::{Digest as _, Sha256};
    use tokio::io::AsyncWriteExt as _;

    use super::{bridge, files_dir, optional_string, with_activity};
    use crate::updates::Manifest;

    pub const SUPPORTED: bool = true;

    /// Whether the user allowed this app to install apps ("unknown sources").
    pub fn can_install() -> bool {
        with_activity(|env, activity| {
            let class = bridge(env, activity)?;
            env.call_static_method(
                &class,
                "canInstallPackages",
                "(Landroid/content/Context;)Z",
                &[JValue::Object(activity)],
            )?
            .z()
        })
        .unwrap_or(false)
    }

    /// The system screen where the user allows it.
    pub fn open_install_settings() {
        let _ = with_activity(|env, activity| {
            let class = bridge(env, activity)?;
            env.call_static_method(
                &class,
                "openInstallSettings",
                "(Landroid/content/Context;)V",
                &[JValue::Object(activity)],
            )?;
            Ok(())
        });
    }

    /// A failure the system installer reported since it was last asked (it runs while the app
    /// is in the background, so `Bridge` keeps it).
    pub fn take_error() -> Option<String> {
        with_activity(|env, activity| {
            let class = bridge(env, activity)?;
            let error = env
                .call_static_method(
                    &class,
                    "takeInstallError",
                    "(Landroid/content/Context;)Ljava/lang/String;",
                    &[JValue::Object(activity)],
                )?
                .l()?;
            optional_string(env, error)
        })
        .ok()
        .flatten()
    }

    fn apk_path() -> PathBuf {
        files_dir().join("update.apk")
    }

    /// Download the APK (reporting progress), check its size and SHA-256, then start the system
    /// installer. Android itself refuses an APK signed with another key.
    pub async fn download_and_install(
        client: &api::Client,
        manifest: &Manifest,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<(), String> {
        let path = apk_path();
        let result = download(client, manifest, &path, &mut progress).await;
        if let Err(error) = result {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(error);
        }
        let path_string = path.to_string_lossy().into_owned();
        let error = with_activity(|env, activity| {
            let class = bridge(env, activity)?;
            let path = env.new_string(&path_string)?;
            let error = env
                .call_static_method(
                    &class,
                    "installApk",
                    "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(activity), JValue::Object(&path)],
                )?
                .l()?;
            optional_string(env, error)
        })?;
        match error {
            None => Ok(()),
            Some(error) => Err(format!("Nie udało się uruchomić instalacji ({error}).")),
        }
    }

    async fn download(
        client: &api::Client,
        manifest: &Manifest,
        path: &std::path::Path,
        progress: &mut impl FnMut(u64, u64),
    ) -> Result<(), String> {
        const NETWORK: &str = "Nie udało się pobrać aktualizacji. Sprawdź połączenie.";
        let response = reqwest::get(client.url(&manifest.url))
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|_| NETWORK.to_owned())?;
        let mut file = tokio::fs::File::create(path)
            .await
            .map_err(|e| format!("Nie można zapisać pliku ({e})."))?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        let mut body = response.bytes_stream();
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|_| NETWORK.to_owned())?;
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Nie można zapisać pliku ({e})."))?;
            done += chunk.len() as u64;
            progress(done, manifest.size);
        }
        file.flush().await.map_err(|e| e.to_string())?;
        let digest: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if done != manifest.size || !digest.eq_ignore_ascii_case(&manifest.sha256) {
            return Err("Pobrany plik jest uszkodzony. Spróbuj ponownie.".to_owned());
        }
        Ok(())
    }
}
