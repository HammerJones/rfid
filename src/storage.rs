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
pub fn export_dir() -> PathBuf {
    if let Some(d) = EXPORT_DIR.get() {
        return d.clone();
    }
    #[cfg(not(target_os = "android"))]
    if let Some(u) = directories::UserDirs::new() {
        if let Some(docs) = u.document_dir() {
            return docs.to_path_buf();
        }
    }
    data_dir()
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
    let dir = export_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(file_name);
    std::fs::write(&path, contents)?;
    Ok(path)
}
