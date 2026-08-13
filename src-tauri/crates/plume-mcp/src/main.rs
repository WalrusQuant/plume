use std::sync::{Arc, Mutex};

use plume_core::storage;
use plume_mcp::path::resolve_db_path;
use plume_mcp::tools;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use rusqlite::Connection;
use serde::Deserialize;

const INSTRUCTIONS: &str = "\
Plume is a local-first AI notebook. Projects are folders. Plans are markdown \
documents of type `plan`. Prefer updating an existing project plan in place \
over creating loose notes. Ideas belong in the inbox (`capture_idea`); sources \
are read-only imports. Do not bury decisions in chat — write them into the plan.";

#[derive(Clone)]
struct PlumeMcp {
    conn: Arc<Mutex<Connection>>,
    #[allow(dead_code)] // read by #[tool_handler]
    tool_router: ToolRouter<Self>,
}

impl PlumeMcp {
    fn new(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
            tool_router: Self::tool_router(),
        }
    }

    fn with_conn<T: serde::Serialize>(
        &self,
        f: impl FnOnce(&Connection) -> plume_core::error::Result<T>,
    ) -> Result<String, String> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned".to_string())?;
        let value = f(&conn).map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListDocumentsArgs {
    /// Folder / project id. Omit to list across the notebook.
    #[serde(default)]
    project_id: Option<String>,
    /// Restrict to a document type (`plan`, `build-log`, `generic`, …).
    #[serde(default, rename = "type")]
    doc_type: Option<String>,
    /// Include Ideas inbox items (hidden by default).
    #[serde(default)]
    include_inbox: bool,
    /// Include imported Sources (hidden by default).
    #[serde(default)]
    include_sources: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct IdArgs {
    id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchArgs {
    query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateProjectArgs {
    name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateDocumentArgs {
    name: String,
    /// Defaults to `plan`.
    #[serde(default, rename = "type")]
    doc_type: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateDocumentArgs {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AppendArgs {
    id: String,
    text: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CaptureIdeaArgs {
    body: String,
    #[serde(default)]
    title: Option<String>,
}

#[tool_router]
impl PlumeMcp {
    #[tool(
        description = "List notebook projects (folders) with plan and document counts. Ideas and sources are not counted."
    )]
    fn list_projects(&self) -> Result<String, String> {
        self.with_conn(tools::list_projects)
    }

    #[tool(
        description = "List documents. Filter by project_id and/or type. Ideas and sources are omitted unless include_inbox / include_sources."
    )]
    fn list_documents(
        &self,
        Parameters(args): Parameters<ListDocumentsArgs>,
    ) -> Result<String, String> {
        self.with_conn(|c| {
            tools::list_documents(
                c,
                args.project_id.as_deref(),
                args.doc_type.as_deref(),
                args.include_inbox,
                args.include_sources,
            )
        })
    }

    #[tool(description = "Read a document's metadata and full markdown body.")]
    fn get_document(&self, Parameters(args): Parameters<IdArgs>) -> Result<String, String> {
        self.with_conn(|c| tools::get_document(c, &args.id))
    }

    #[tool(description = "Keyword search across notes and sources. Ideas are excluded. Uses local FTS, not embeddings.")]
    fn search_notes(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<String, String> {
        self.with_conn(|c| tools::search_notes(c, &args.query))
    }

    #[tool(description = "Create a project (folder) on the notebook shelf.")]
    fn create_project(
        &self,
        Parameters(args): Parameters<CreateProjectArgs>,
    ) -> Result<String, String> {
        self.with_conn(|c| tools::create_project(c, &args.name))
    }

    #[tool(
        description = "Create a document. type defaults to plan. Pass project_id to file it in a project. Empty plan bodies get a starter outline."
    )]
    fn create_document(
        &self,
        Parameters(args): Parameters<CreateDocumentArgs>,
    ) -> Result<String, String> {
        self.with_conn(|c| {
            tools::create_document(
                c,
                &args.name,
                args.doc_type.as_deref(),
                args.content.as_deref(),
                args.project_id.as_deref(),
            )
        })
    }

    #[tool(description = "Rename a document and/or replace its markdown body. Require id.")]
    fn update_document(
        &self,
        Parameters(args): Parameters<UpdateDocumentArgs>,
    ) -> Result<String, String> {
        self.with_conn(|c| tools::update_document(c, &args.id, args.name.as_deref(), args.content.as_deref()))
    }

    #[tool(description = "Append markdown to a document (good for build logs). Adds a trailing newline.")]
    fn append_to_document(
        &self,
        Parameters(args): Parameters<AppendArgs>,
    ) -> Result<String, String> {
        self.with_conn(|c| tools::append_to_document(c, &args.id, &args.text))
    }

    #[tool(description = "Capture a quick idea into the Inbox. Not a project document; not keyword-searchable.")]
    fn capture_idea(
        &self,
        Parameters(args): Parameters<CaptureIdeaArgs>,
    ) -> Result<String, String> {
        self.with_conn(|c| tools::capture_idea(c, args.title.as_deref(), &args.body))
    }
}

#[tool_handler]
impl ServerHandler for PlumeMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(INSTRUCTIONS)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = match resolve_db_path(std::env::args()) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{msg}");
            let is_help = msg.contains("Usage:");
            std::process::exit(if is_help { 0 } else { 2 });
        }
    };
    if path == plume_core::default_db_path() {
        let _ = plume_core::migrate_legacy_data_dir(&plume_core::default_data_dir());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&path)?;
    storage::init(&conn)?;
    let service = PlumeMcp::new(conn).serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
