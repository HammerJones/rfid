//! Where the budget is saved and where exports go.

use crate::model::Budget;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static EXPORT_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Android sets these from the activity at startup.
pub fn set_dirs(data: PathBuf, export: PathBuf) {
    let _ = DATA_DIR.set(data);
    let _ = EXPORT_DIR.set(export);
}

pub fn data_dir() -> PathBuf {
    if let Some(d) = DATA_DIR.get() {
        return d.clone();
    }
    #[cfg(not(target_os = "android"))]
    if let Some(p) = directories::ProjectDirs::from("com", "HammerJones", "Budget") {
        return p.data_dir().to_path_buf();
    }
    PathBuf::from(".")
}

/// Default folder for exported CSVs.
/// Android: Download/Budget on shared storage, so files are easy to find.
/// Desktop: Documents.
pub fn export_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        return PathBuf::from(ANDROID_STORAGE).join("Download").join("Budget");
    }
    #[cfg(not(target_os = "android"))]
    {
        if let Some(d) = EXPORT_DIR.get() {
            return d.clone();
        }
        if let Some(u) = directories::UserDirs::new() {
            if let Some(docs) = u.document_dir() {
                return docs.to_path_buf();
            }
        }
        data_dir()
    }
}

#[cfg(target_os = "android")]
const ANDROID_STORAGE: &str = "/storage/emulated/0";

/// Folders the in-app file picker offers as starting points.
pub fn picker_roots() -> Vec<(String, PathBuf)> {
    let mut roots = Vec::new();
    #[cfg(target_os = "android")]
    {
        let base = PathBuf::from(ANDROID_STORAGE);
        roots.push(("Downloads".to_string(), base.join("Download")));
        roots.push(("Documents".to_string(), base.join("Documents")));
        roots.push(("Budget exports".to_string(), export_dir()));
        roots.push(("Phone storage".to_string(), base));
    }
    #[cfg(not(target_os = "android"))]
    if let Some(u) = directories::UserDirs::new() {
        if let Some(d) = u.download_dir() {
            roots.push(("Downloads".to_string(), d.to_path_buf()));
        }
        if let Some(d) = u.document_dir() {
            roots.push(("Documents".to_string(), d.to_path_buf()));
        }
        roots.push(("Home".to_string(), u.home_dir().to_path_buf()));
    }
    roots
}
pub fn budget_path() -> PathBuf {
    data_dir().join("budget.json")
}

pub fn load() -> Option<Budget> {
    load_from(&budget_path())
}

pub fn load_from(path: &Path) -> Option<Budget> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Atomic save: write a temp file, then rename over the old one.
pub fn save(budget: &Budget) -> std::io::Result<()> {
    let path = budget_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(budget).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)
}

pub fn write_export(file_name: &str, contents: &str) -> std::io::Result<PathBuf> {
    let primary = export_dir();
    let attempt = |dir: &Path| -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(file_name);
        std::fs::write(&path, contents)?;
        Ok(path)
    };
    match attempt(&primary) {
        Ok(p) => Ok(p),
        // No permission for shared storage: fall back to the app's own folder.
        Err(e) => match EXPORT_DIR.get() {
            Some(fallback) if fallback != &primary => attempt(fallback),
            _ => Err(e),
        },
    }
}
