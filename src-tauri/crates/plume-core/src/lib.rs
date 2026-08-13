pub mod embed_blob;
pub mod error;
pub mod storage;

use std::path::{Path, PathBuf};

/// Matches `identifier` in `src-tauri/tauri.conf.json`. Reverse-DNS of plumemd.com.
pub const APP_IDENTIFIER: &str = "com.plumemd.app";
/// Pre-rebrand data dir. Copied forward once if the new dir has no notebook yet.
pub const LEGACY_APP_IDENTIFIER: &str = "com.adamwickwire.markdown";
pub const DB_FILENAME: &str = "markdown.db";

fn data_dir_root() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from("."))
}

pub fn default_data_dir() -> PathBuf {
    data_dir_root().join(APP_IDENTIFIER)
}

pub fn legacy_data_dir() -> PathBuf {
    data_dir_root().join(LEGACY_APP_IDENTIFIER)
}

/// Default notebook database path (same directory Tauri's `app_data_dir` uses).
/// Override at the MCP boundary with `--db` or `PLUME_DB`.
pub fn default_db_path() -> PathBuf {
    default_data_dir().join(DB_FILENAME)
}

/// Ensure the current data dir exists and, if it has no notebook yet, copy the
/// pre-rebrand Application Support folder (db, WAL, embed models, dev-keys).
/// The old folder is left in place. Returns true when a copy ran.
pub fn migrate_legacy_data_dir(dest_dir: &Path) -> std::io::Result<bool> {
    migrate_from_legacy(dest_dir, &legacy_data_dir())
}

/// Same as `default_db_path`, after a best-effort legacy copy.
pub fn prepare_default_db_path() -> PathBuf {
    let dir = default_data_dir();
    let _ = migrate_legacy_data_dir(&dir);
    dir.join(DB_FILENAME)
}

pub fn migrate_from_legacy(dest_dir: &Path, legacy_dir: &Path) -> std::io::Result<bool> {
    if dest_dir.join(DB_FILENAME).exists() {
        return Ok(false);
    }
    if !legacy_dir.join(DB_FILENAME).exists() {
        return Ok(false);
    }
    copy_dir_missing(legacy_dir, dest_dir)?;
    Ok(true)
}

/// Copy `src` into `dst`, skipping names that already exist at the destination.
fn copy_dir_missing(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if to.exists() {
            continue;
        }
        if entry.file_type()?.is_dir() {
            copy_dir_missing(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_db_path_uses_app_identifier() {
        let path = default_db_path();
        assert_eq!(path.file_name().and_then(|s| s.to_str()), Some(DB_FILENAME));
        assert!(path
            .components()
            .any(|c| c.as_os_str() == APP_IDENTIFIER));
        assert!(!path
            .components()
            .any(|c| c.as_os_str() == LEGACY_APP_IDENTIFIER));
    }

    #[test]
    fn migrate_copies_legacy_notebook_once() {
        let tmp = std::env::temp_dir().join(format!("plume-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let legacy = tmp.join("legacy");
        let dest = tmp.join("dest");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join(DB_FILENAME), b"notebook").unwrap();
        std::fs::write(legacy.join("dev-keys.json"), b"{}").unwrap();

        assert!(migrate_from_legacy(&dest, &legacy).unwrap());
        assert_eq!(std::fs::read(dest.join(DB_FILENAME)).unwrap(), b"notebook");
        assert!(dest.join("dev-keys.json").exists());
        // second run is a no-op even if legacy changes
        std::fs::write(legacy.join(DB_FILENAME), b"changed").unwrap();
        assert!(!migrate_from_legacy(&dest, &legacy).unwrap());
        assert_eq!(std::fs::read(dest.join(DB_FILENAME)).unwrap(), b"notebook");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn migrate_skips_when_legacy_missing() {
        let tmp = std::env::temp_dir().join(format!("plume-migrate-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let dest = tmp.join("dest");
        assert!(!migrate_from_legacy(&dest, &tmp.join("nope")).unwrap());
        assert!(!dest.join(DB_FILENAME).exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
