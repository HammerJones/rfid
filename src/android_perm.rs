//! Android "All files access" helpers, via JNI.
//!
//! Android 11+ only lets an app list other apps' files in Downloads (like a CSV your browser
//! saved) if the user turns on "All files access" for it. These helpers check that and open the
//! right settings screen.

use jni::objects::{JObject, JValue};
use jni::JavaVM;

const PACKAGE: &str = "com.hammerjones.budget";
const FLAG_ACTIVITY_NEW_TASK: i32 = 0x1000_0000;

fn with_env<R>(f: impl FnOnce(&mut jni::JNIEnv, &JObject) -> jni::errors::Result<R>) -> Result<R, String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    // The activity is a global reference owned by android-activity; we only borrow it.
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    let result = f(&mut env, &activity);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result.map_err(|e| e.to_string())
}

fn sdk_int(env: &mut jni::JNIEnv) -> jni::errors::Result<i32> {
    env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?.i()
}

/// True when the app can read any file in shared storage.
pub fn has_all_files_access() -> bool {
    with_env(|env, _| {
        if sdk_int(env)? < 30 {
            // Android 10 and older: the regular storage permission from the manifest applies.
            return Ok(true);
        }
        env.call_static_method("android/os/Environment", "isExternalStorageManager", "()Z", &[])?.z()
    })
    .unwrap_or(false)
}

/// Open the "All files access" switch for this app (falls back to the general list).
pub fn open_all_files_settings() -> Result<(), String> {
    let direct = with_env(|env, activity| {
        let action = env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?;
        let uri_text = env.new_string(format!("package:{PACKAGE}"))?;
        let uri = env
            .call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&uri_text)])?
            .l()?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V", &[JValue::Object(&action), JValue::Object(&uri)])?;
        env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)])?;
        env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(())
    });
    if direct.is_ok() {
        return Ok(());
    }
    // Some phones don't support the per-app screen; open the full list instead.
    with_env(|env, activity| {
        let action = env.new_string("android.settings.MANAGE_ALL_FILES_ACCESS_PERMISSION")?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
        env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)])?;
        env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(())
    })
}
