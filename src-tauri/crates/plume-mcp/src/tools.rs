use plume_core::error::{Error, Result};
use plume_core::storage::{self, DocType, Document, Folder};
use rusqlite::Connection;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub plan_count: usize,
    pub doc_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocSummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub doc_type: DocType,
    pub project_id: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocFull {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub doc_type: DocType,
    pub project_id: Option<String>,
    pub title_explicit: bool,
    pub created_at: String,
    pub updated_at: String,
    pub content: String,
}

fn is_tree_hidden(t: DocType) -> bool {
    matches!(t, DocType::Idea | DocType::Source)
}

fn default_plan_body(name: &str) -> String {
    format!(
        "# Plan: {name}\n\n## Why\n\n## Shape of done\n\n## Steps\n\n1.\n\n## Open questions\n\n-\n"
    )
}

pub fn list_projects(conn: &Connection) -> Result<Vec<ProjectSummary>> {
    let folders = storage::list_folders(conn)?;
    let docs = storage::list_documents(conn)?;
    Ok(folders
        .into_iter()
        .map(|f| {
            let in_project: Vec<_> = docs
                .iter()
                .filter(|d| d.folder_id.as_deref() == Some(f.id.as_str()) && !is_tree_hidden(d.doc_type))
                .collect();
            ProjectSummary {
                id: f.id,
                name: f.name,
                active: f.active,
                plan_count: in_project.iter().filter(|d| d.doc_type == DocType::Plan).count(),
                doc_count: in_project.len(),
            }
        })
        .collect())
}

pub fn list_documents(
    conn: &Connection,
    project_id: Option<&str>,
    type_filter: Option<&str>,
    include_inbox: bool,
    include_sources: bool,
) -> Result<Vec<DocSummary>> {
    let want_type = match type_filter {
        Some(s) => Some(DocType::parse(s)?),
        None => None,
    };
    let docs = storage::list_documents(conn)?;
    Ok(docs
        .into_iter()
        .filter(|d| match project_id {
            Some(pid) => d.folder_id.as_deref() == Some(pid),
            None => true,
        })
        .filter(|d| want_type.is_none_or(|t| d.doc_type == t))
        .filter(|d| match d.doc_type {
            DocType::Idea => include_inbox,
            DocType::Source => include_sources,
            _ => true,
        })
        .map(|d| DocSummary {
            id: d.id,
            name: d.name,
            doc_type: d.doc_type,
            project_id: d.folder_id,
            updated_at: d.updated_at,
        })
        .collect())
}

pub fn get_document(conn: &Connection, id: &str) -> Result<DocFull> {
    let meta = storage::list_documents(conn)?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or(Error::NotFound("document"))?;
    let content = storage::get_document_content(conn, id)?;
    Ok(DocFull {
        id: meta.id,
        name: meta.name,
        doc_type: meta.doc_type,
        project_id: meta.folder_id,
        title_explicit: meta.title_explicit,
        created_at: meta.created_at,
        updated_at: meta.updated_at,
        content,
    })
}

pub fn search_notes(conn: &Connection, query: &str) -> Result<Vec<storage::SearchHit>> {
    storage::search_documents(conn, query)
}

pub fn create_project(conn: &Connection, name: &str) -> Result<Folder> {
    storage::create_folder(conn, name)
}

pub fn create_document(
    conn: &Connection,
    name: &str,
    type_name: Option<&str>,
    content: Option<&str>,
    project_id: Option<&str>,
) -> Result<Document> {
    let doc_type = match type_name {
        Some(s) => DocType::parse(s)?,
        None => DocType::Plan,
    };
    let body = match content {
        Some(c) => Some(c.to_string()),
        None if doc_type == DocType::Plan => Some(default_plan_body(name)),
        None => None,
    };
    storage::create_document_in_folder(
        conn,
        name,
        Some(doc_type),
        body.as_deref(),
        project_id,
    )
}

pub fn update_document(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    content: Option<&str>,
) -> Result<Document> {
    if name.is_none() && content.is_none() {
        return Err(Error::InvalidInput(
            "update_document needs name and/or content".into(),
        ));
    }
    if let Some(content) = content {
        storage::save_document_content(conn, id, content)?;
    }
    if let Some(name) = name {
        return storage::rename_document(conn, id, name);
    }
    storage::list_documents(conn)?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or(Error::NotFound("document"))
}

pub fn append_to_document(conn: &Connection, id: &str, text: &str) -> Result<Document> {
    if text.is_empty() {
        return Err(Error::InvalidInput("append text is empty".into()));
    }
    let mut body = storage::get_document_content(conn, id)?;
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str(text);
    if !text.ends_with('\n') {
        body.push('\n');
    }
    storage::save_document_content(conn, id, &body)?;
    storage::list_documents(conn)?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or(Error::NotFound("document"))
}

pub fn capture_idea(conn: &Connection, title: Option<&str>, body: &str) -> Result<Document> {
    let derived = body.lines().find(|l| !l.trim().is_empty()).unwrap_or("Idea");
    let name = title.filter(|t| !t.trim().is_empty()).unwrap_or(derived);
    let explicit = title.map(|t| !t.trim().is_empty()).unwrap_or(false);
    let doc = storage::create_document(conn, name, Some(DocType::Idea), Some(body))?;
    storage::update_idea_name(conn, &doc.id, name, explicit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        storage::init(&conn).unwrap();
        conn
    }

    #[test]
    fn create_project_then_plan_lists_under_that_project() {
        let conn = test_conn();
        let project = create_project(&conn, "MCP").unwrap();
        let plan = create_document(&conn, "Ship MCP", Some("plan"), None, Some(&project.id)).unwrap();
        assert_eq!(plan.doc_type, DocType::Plan);
        assert_eq!(plan.folder_id.as_deref(), Some(project.id.as_str()));
        assert!(plan.title_explicit);
        assert!(get_document(&conn, &plan.id).unwrap().content.contains("# Plan:"));

        let listed = list_documents(&conn, Some(&project.id), Some("plan"), false, false).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, plan.id);

        let projects = list_projects(&conn).unwrap();
        assert_eq!(projects[0].plan_count, 1);
        assert_eq!(projects[0].doc_count, 1);
    }

    #[test]
    fn update_then_get_roundtrip() {
        let conn = test_conn();
        let doc = create_document(&conn, "P", Some("plan"), Some("v1"), None).unwrap();
        update_document(&conn, &doc.id, Some("Renamed"), Some("v2")).unwrap();
        let got = get_document(&conn, &doc.id).unwrap();
        assert_eq!(got.name, "Renamed");
        assert_eq!(got.content, "v2");
    }

    #[test]
    fn append_concatenates() {
        let conn = test_conn();
        let doc = create_document(&conn, "Log", Some("build-log"), Some("day 1"), None).unwrap();
        append_to_document(&conn, &doc.id, "day 2").unwrap();
        assert_eq!(get_document(&conn, &doc.id).unwrap().content, "day 1\nday 2\n");
    }

    #[test]
    fn search_finds_plan_body() {
        let conn = test_conn();
        create_document(
            &conn,
            "Searchable",
            Some("plan"),
            Some("unique-xylophone-token in the plan"),
            None,
        )
        .unwrap();
        let hits = search_notes(&conn, "xylophone").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Searchable");
    }

    #[test]
    fn ideas_hidden_from_default_list() {
        let conn = test_conn();
        capture_idea(&conn, Some("Spark"), "a half-formed thought").unwrap();
        assert!(list_documents(&conn, None, None, false, false).unwrap().is_empty());
        let inbox = list_documents(&conn, None, None, true, false).unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].doc_type, DocType::Idea);
        assert!(search_notes(&conn, "thought").unwrap().is_empty());
    }

    #[test]
    fn unknown_id_is_not_found() {
        let conn = test_conn();
        assert!(matches!(get_document(&conn, "nope"), Err(Error::NotFound("document"))));
        assert!(matches!(
            update_document(&conn, "nope", None, Some("x")),
            Err(Error::NotFound("document"))
        ));
        assert!(matches!(
            append_to_document(&conn, "nope", "x"),
            Err(Error::NotFound("document"))
        ));
    }

    #[test]
    fn missing_folder_is_not_found() {
        let conn = test_conn();
        let err = create_document(&conn, "P", Some("plan"), None, Some("missing")).unwrap_err();
        assert!(matches!(err, Error::NotFound("folder")));
    }

    #[test]
    fn default_type_is_plan() {
        let conn = test_conn();
        let doc = create_document(&conn, "Untitled plan", None, None, None).unwrap();
        assert_eq!(doc.doc_type, DocType::Plan);
    }
}
