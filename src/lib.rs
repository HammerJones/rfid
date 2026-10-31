//! Budget planner: scheduled expenses, monthly summaries, CSV import/export.

pub mod app;
pub mod bank;
pub mod csvio;
pub mod model;
pub mod storage;
pub mod theme;

pub const APP_NAME: &str = "Budget";

/// Desktop entry point (Windows, Linux, macOS).
#[cfg(not(target_os = "android"))]
pub fn run_desktop() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([1000.0, 760.0])
            .with_min_inner_size([360.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(APP_NAME, options, Box::new(|cc| Ok(Box::new(app::BudgetApp::new(cc)))))
}

/// Android entry point.
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android_app: winit::platform::android::activity::AndroidApp) {
    let data = android_app
        .internal_data_path()
        .unwrap_or_else(|| std::path::PathBuf::from("/data/local/tmp"));
    // App-specific external storage (Android/data/<package>/files) is reachable over USB.
    let export = android_app.external_data_path().unwrap_or_else(|| data.clone());
    storage::set_dirs(data, export);

    let options = eframe::NativeOptions {
        android_app: Some(android_app),
        ..Default::default()
    };
    let _ = eframe::run_native(APP_NAME, options, Box::new(|cc| Ok(Box::new(app::BudgetApp::new(cc)))));
}
