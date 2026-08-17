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
///
/// Prefer [`resolve_notebook_db_path`] when opening the user's notebook — a
/// rebrand can leave a stub at this path while the living data stays under
/// [`LEGACY_APP_IDENTIFIER`].
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

/// Path of the notebook the desktop app and `plume-mcp` must share.
///
/// After the identifier change, a first launch under the new id can create an
/// empty-ish `markdown.db` and then skip the one-shot copy (dest already
/// exists). Agents that still open the pre-rebrand folder keep writing the
/// living notebook next door. Pick the richer of the two so both processes
/// see the same projects.
pub fn resolve_notebook_db_path() -> PathBuf {
    resolve_notebook_db_path_from(&default_data_dir(), &legacy_data_dir())
}

pub fn resolve_notebook_db_path_from(dest_dir: &Path, legacy_dir: &Path) -> PathBuf {
    let dest_db = dest_dir.join(DB_FILENAME);
    let legacy_db = legacy_dir.join(DB_FILENAME);

    if !dest_db.exists() {
        if legacy_db.exists() {
            let _ = migrate_from_legacy(dest_dir, legacy_dir);
            if dest_dir.join(DB_FILENAME).exists() {
                return dest_dir.join(DB_FILENAME);
            }
            return legacy_db;
        }
        return dest_db;
    }
    if !legacy_db.exists() {
        return dest_db;
    }

    match (notebook_score(&dest_db), notebook_score(&legacy_db)) {
        (Some(dest), Some(legacy)) if legacy > dest => legacy_db,
        _ => dest_db,
    }
}

/// (folders, documents, max updated_at). Missing/unreadable notebooks score
/// nothing so the other side wins.
fn notebook_score(path: &Path) -> Option<(i64, i64, String)> {
    let conn = rusqlite::Connection::open(path).ok()?;
    let _ = conn.busy_timeout(std::time::Duration::from_millis(250));
    let folders: i64 = conn
        .query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0))
        .unwrap_or(0);
    let docs: i64 = conn
        .query_row("SELECT COUNT(*) FROM documents", [], |r| r.get(0))
        .unwrap_or(0);
    let newest: String = conn
        .query_row(
            "SELECT COALESCE(MAX(updated_at), '') FROM documents",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    Some((folders, docs, newest))
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

    fn seed_notebook(dir: &Path, folders: usize, docs: usize) {
        std::fs::create_dir_all(dir).unwrap();
        let conn = rusqlite::Connection::open(dir.join(DB_FILENAME)).unwrap();
        storage::init(&conn).unwrap();
        for i in 0..folders {
            storage::create_folder(&conn, &format!("Folder {i}")).unwrap();
        }
        for i in 0..docs {
            storage::create_document(&conn, &format!("Doc {i}"), None, None).unwrap();
        }
    }

    #[test]
    fn resolve_prefers_current_identifier_when_it_is_the_living_notebook() {
        let tmp = std::env::temp_dir().join(format!("plume-resolve-dest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let dest = tmp.join("dest");
        let legacy = tmp.join("legacy");
        seed_notebook(&dest, 2, 4);
        seed_notebook(&legacy, 1, 1);
        assert_eq!(
            resolve_notebook_db_path_from(&dest, &legacy),
            dest.join(DB_FILENAME)
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_uses_legacy_when_current_identifier_is_a_stub() {
        let tmp = std::env::temp_dir().join(format!("plume-resolve-legacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let dest = tmp.join("dest");
        let legacy = tmp.join("legacy");
        seed_notebook(&dest, 0, 5);
        seed_notebook(&legacy, 7, 12);
        assert_eq!(
            resolve_notebook_db_path_from(&dest, &legacy),
            legacy.join(DB_FILENAME)
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_migrates_legacy_when_dest_is_missing() {
        let tmp = std::env::temp_dir().join(format!("plume-resolve-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let dest = tmp.join("dest");
        let legacy = tmp.join("legacy");
        seed_notebook(&legacy, 1, 1);
        let path = resolve_notebook_db_path_from(&dest, &legacy);
        assert_eq!(path, dest.join(DB_FILENAME));
        assert!(dest.join(DB_FILENAME).exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
