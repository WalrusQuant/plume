use std::path::PathBuf;

/// `--db PATH`, else `PLUME_DB`, else the standard app-data notebook.
pub fn resolve_db_path(args: impl IntoIterator<Item = String>) -> Result<PathBuf, String> {
    let args: Vec<String> = args.into_iter().collect();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--db" {
            let path = iter
                .next()
                .ok_or_else(|| "missing path after --db".to_string())?;
            return Ok(PathBuf::from(path));
        }
        if let Some(path) = arg.strip_prefix("--db=") {
            return Ok(PathBuf::from(path));
        }
        if arg == "--help" || arg == "-h" {
            return Err(usage().to_string());
        }
    }
    if let Ok(path) = std::env::var("PLUME_DB") {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    Ok(plume_core::default_db_path())
}

pub fn usage() -> &'static str {
    "plume-mcp — stdio MCP server for the Plume notebook\n\n\
     Usage: plume-mcp [--db PATH]\n\n\
     Default database: the same markdown.db Plume uses in Application Support.\n\
     Override with --db or the PLUME_DB environment variable.\n\
     Logs must not go to stdout (MCP JSON-RPC)."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_wins_over_env() {
        let path = resolve_db_path(["plume-mcp".into(), "--db".into(), "/tmp/x.db".into()]).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/x.db"));
    }

    #[test]
    fn equals_form() {
        let path = resolve_db_path(["plume-mcp".into(), "--db=/tmp/y.db".into()]).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/y.db"));
    }

    #[test]
    fn missing_db_value_errors() {
        assert!(resolve_db_path(["plume-mcp".into(), "--db".into()]).is_err());
    }

    #[test]
    fn default_points_at_app_identifier() {
        let path = resolve_db_path(["plume-mcp".into()]).unwrap();
        assert_eq!(
            path.file_name().and_then(|s| s.to_str()),
            Some(plume_core::DB_FILENAME)
        );
    }
}
