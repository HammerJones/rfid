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

pub fn write_export(file_name: &str, contents: impl AsRef<[u8]>) -> std::io::Result<PathBuf> {
    let primary = export_dir();
    let attempt = |dir: &Path| -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(file_name);
        std::fs::write(&path, contents.as_ref())?;
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

// ---------------------------------------------------------------- Backups
//
// The app's own data folder is wiped if the app is uninstalled. Backups go to Download/Budget
// (Android) or the Documents folder (desktop), which survive an uninstall.

pub const AUTO_BACKUP_NAME: &str = "Budget Auto-Backup.json";

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Backup {
    pub version: u32,
    pub created: String,
    pub budget: Budget,
    /// Logged purchases (purchases.tsv), so the daily budget history comes back too.
    #[serde(default)]
    pub purchases: String,
}

pub fn backup_json(budget: &Budget) -> String {
    let purchases = std::fs::read_to_string(data_dir().join(crate::daily::PURCHASES_FILE)).unwrap_or_default();
    let b = Backup {
        version: 1,
        created: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        budget: budget.clone(),
        purchases,
    };
    serde_json::to_string_pretty(&b).unwrap_or_default()
}

/// Read a backup file (or a plain budget.json). Returns the budget, logged purchases and when it was made.
pub fn parse_backup(text: &str) -> Result<(Budget, Option<String>, String), String> {
    if let Ok(b) = serde_json::from_str::<Backup>(text) {
        return Ok((b.budget, Some(b.purchases).filter(|p| !p.is_empty()), b.created));
    }
    serde_json::from_str::<Budget>(text)
        .map(|b| (b, None, String::new()))
        .map_err(|_| "That file isn't a Budget backup.".to_string())
}

pub fn auto_backup_path() -> PathBuf {
    export_dir().join(AUTO_BACKUP_NAME)
}

/// Keep an up-to-date copy outside the app's private folder (Android only; desktop data isn't lost on reinstall).
pub fn write_auto_backup(budget: &Budget) -> std::io::Result<()> {
    #[cfg(target_os = "android")]
    {
        let dir = export_dir();
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join("Budget Auto-Backup.json.tmp");
        std::fs::write(&tmp, backup_json(budget))?;
        std::fs::rename(tmp, auto_backup_path())?;
    }
    #[cfg(not(target_os = "android"))]
    let _ = budget;
    Ok(())
}

/// Replace the current data with a backup. The data being replaced is kept as budget-before-restore.json.
pub fn restore(budget: &Budget, purchases: Option<&str>) -> std::io::Result<()> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    if let Ok(old) = std::fs::read(budget_path()) {
        let _ = std::fs::write(dir.join("budget-before-restore.json"), old);
    }
    save(budget)?;
    if let Some(p) = purchases {
        std::fs::write(dir.join(crate::daily::PURCHASES_FILE), p)?;
    }
    Ok(())
}

#[cfg(test)]
mod backup_tests {
    use super::*;

    #[test]
    fn backup_round_trip_and_plain_budget() {
        let mut b = Budget::default();
        b.settings.hourly_rate = 31.5;
        let (back, _, created) = parse_backup(&backup_json(&b)).unwrap();
        assert_eq!(back.settings.hourly_rate, 31.5);
        assert!(!created.is_empty());
        let plain = serde_json::to_string(&b).unwrap();
        assert_eq!(parse_backup(&plain).unwrap().0.settings.hourly_rate, 31.5);
        assert!(parse_backup("name,amount\nx,1").is_err());
    }
}
