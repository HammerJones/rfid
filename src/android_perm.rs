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

/// True when "Notification access" is on for this app (needed to log Wallet/PayPal purchases).
pub fn has_notification_access() -> bool {
    with_env(|env, activity| {
        let resolver = env.call_method(activity, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
        let key = env.new_string("enabled_notification_listeners")?;
        let value = env
            .call_static_method(
                "android/provider/Settings$Secure",
                "getString",
                "(Landroid/content/ContentResolver;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&resolver), JValue::Object(&key)],
            )?
            .l()?;
        if value.is_null() {
            return Ok(false);
        }
        let text: String = env.get_string(&jni::objects::JString::from(value))?.into();
        Ok(text.contains(PACKAGE))
    })
    .unwrap_or(false)
}

/// Open the Notification access screen.
pub fn open_notification_settings() -> Result<(), String> {
    with_env(|env, activity| {
        let action = env.new_string("android.settings.ACTION_NOTIFICATION_LISTENER_SETTINGS")?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
        env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)])?;
        env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(())
    })
}

/// Open this app's App info page (where "Allow restricted settings" lives on Android 13+).
pub fn open_app_info() -> Result<(), String> {
    with_env(|env, activity| {
        let action = env.new_string("android.settings.APPLICATION_DETAILS_SETTINGS")?;
        let uri_text = env.new_string(format!("package:{PACKAGE}"))?;
        let uri = env
            .call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&uri_text)])?
            .l()?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V", &[JValue::Object(&action), JValue::Object(&uri)])?;
        env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)])?;
        env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(())
    })
}

/// True when the app may post notifications (always true before Android 13).
pub fn has_notification_permission() -> bool {
    with_env(|env, activity| {
        if sdk_int(env)? < 33 {
            return Ok(true);
        }
        let p = env.new_string("android.permission.POST_NOTIFICATIONS")?;
        let r = env.call_method(activity, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&p)])?.i()?;
        Ok(r == 0)
    })
    .unwrap_or(false)
}

/// Open this app's notification settings, where notifications can be allowed.
pub fn open_app_notification_settings() -> Result<(), String> {
    with_env(|env, activity| {
        let action = env.new_string("android.settings.APP_NOTIFICATION_SETTINGS")?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
        let key = env.new_string("android.provider.extra.APP_PACKAGE")?;
        let pkg = env.new_string(PACKAGE)?;
        env.call_method(
            &intent,
            "putExtra",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&key), JValue::Object(&pkg)],
        )?;
        env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)])?;
        env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(())
    })
}

/// Tell the home screen widget to redraw (after widget.json changed).
pub fn refresh_widget() {
    let _ = with_env(|env, activity| {
        let action = env.new_string(format!("{PACKAGE}.REFRESH_WIDGET"))?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
        let pkg = env.new_string(PACKAGE)?;
        let cls = env.new_string(format!("{PACKAGE}.BudgetWidget"))?;
        env.call_method(
            &intent,
            "setClassName",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&pkg), JValue::Object(&cls)],
        )?;
        env.call_method(activity, "sendBroadcast", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(())
    });
}
