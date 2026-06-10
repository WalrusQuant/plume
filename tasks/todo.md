# Plan: Markdown writing app — "write once, publish everywhere" (Tauri desktop)

> Status: **APPROVED — ready for M0.** All decisions resolved (see bottom). No code written yet.

## Product

A **local-first desktop markdown writing app with an AI writing partner that exports your work — adapted per platform — to everywhere you publish.**

- **Audience:** content creators who write in markdown and publish to many places (blogs, newsletters, LinkedIn, X, Substack/Medium/Ghost/beehiiv).
- **Build lead:** the editor + AI experience (daily-use, sticky).
- **Marketing lead:** "write once, publish everywhere" (the export — the pain reliever).
- **One engine, three uses:** comrak parses markdown → AST → (a) live preview, (b) AI context, (c) every export renderer.

This supersedes the old web app **AgentDocs** (`~/GitHub/markdown-collab`). We salvage its editor craft + design system and drop its collaboration/server stack.

## Stack (user's preferred)

- **Shell:** Tauri v2 (Rust backend, web frontend)
- **Frontend:** SvelteKit + adapter-static (SSR off), Svelte 5 runes, TypeScript, CodeMirror 6
- **Backend:** Rust — rusqlite (bundled, WAL), comrak (markdown AST + render), reqwest (rustls-tls) for AI + publishing, serde, tokio, chrono, anyhow, thiserror 2
- **Styling:** the ported CSS-custom-properties design system (light/dark) — no Tailwind
- **Build:** pnpm, Vite (via SvelteKit), Tauri CLI

## Architecture

```
                    SvelteKit webview (Svelte 5 runes)
   ┌──────────────────────────────────────────────────────────────┐
   │  Editor (CodeMirror 6)   Live Preview   AI panel   Export panel │
   │        │                      ▲             │           │       │
   └────────┼──────────────────────┼─────────────┼───────────┼───────┘
       invoke()              Tauri events     invoke()    invoke()
            │                      │             │           │
   ┌────────▼──────────────────────┴─────────────▼───────────▼───────┐
   │                       Rust backend (Tauri commands)              │
   │  rusqlite (docs/folders + content)   comrak AST → Renderer trait │
   │  reqwest streaming AI (key held in Rust)   publishing clients    │
   └──────────────────────────────────────────────────────────────────┘
```

### SQLite schema (rusqlite, WAL)
```sql
CREATE TABLE IF NOT EXISTS folders (
  id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL,
  parent_id TEXT REFERENCES folders(id) ON DELETE SET NULL,  -- future-proofing only; v1 UI is flat (decision 1)
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS documents (
  id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL,
  type TEXT NOT NULL DEFAULT 'generic',      -- no CHECK: SQLite can't alter one later; enum validated in Rust
  folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
  content TEXT NOT NULL DEFAULT '',          -- NEW: was Yjs-only, now first-class
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
```
IDs: UUID v4 (`uuid` crate). Timestamps: RFC3339 (`chrono::Utc::now().to_rfc3339()`). `PRAGMA foreign_keys = ON`.

Document `type` is a Rust enum (serde round-trip, exhaustive match for template lookup): creator types **`blog-post`, `newsletter`, `linkedin-post`, `x-thread`** lead; agent-file types `skill`, `claude-md`, `system-prompt`, `runbook` kept as the bonus tier (decision 5); `generic` is the default. Adding a type later = one enum variant, no migration.

### Tauri commands (replace the old REST API 1:1, + new content commands)
- Documents: `list_documents`, `create_document(name, doc_type?)`, `rename_document(id, name)`, `move_document(id, folder_id?)`, `delete_document(id)`, **`get_document_content(id)`**, **`save_document_content(id, content)`**
- Folders: `list_folders`, `create_folder(name)`, `rename_folder(id, name)`, `delete_folder(id)`
- Preview: `render_preview(content) -> html` (comrak — same engine as export, decision 7)
- AI: `send_assistant_message(...)` (streams via events), `stop_assistant()`
- Export: `list_export_targets()`, `export_document(id|content, target_id) -> clipboard text | written file path`

### AI layer (key never touches the webview)
- Rust holds the API key (OS keychain via `keyring` crate, or encrypted SQLite — see decisions).
- Rust builds the system prompt (injects current doc), calls Anthropic `/v1/messages` (`anthropic-version: 2023-06-01`) or OpenRouter (`/api/v1/chat/completions`) with `stream: true` via reqwest.
- Parses SSE; emits `assistant:token` / `assistant:done` / `assistant:error` Tauri events. `stop_assistant` cancels via a cancellation token.
- Svelte side: rune store holds `messages` + `isStreaming`, appends on token events. Prompt-building logic ported from old `useAssistant.ts`.
- **Model default: `claude-opus-4-8`** (the old `claude-sonnet-4-20250514` default is stale). Likely mix: a fast model for inline edits, a stronger one for big "adapt everywhere" jobs. Pin specifics against the `claude-api` skill at build time.

### Export engine (the net-new "everything out")
```rust
enum Delivery { Clipboard, File { ext: &'static str }, Publish }
trait Renderer {
  fn id(&self) -> &str;
  fn label(&self) -> &str;
  fn delivery(&self) -> Delivery;
  fn render(&self, ast: &AstNode, ctx: &RenderCtx) -> RenderOutput;
}
```
*(Signature is illustrative — comrak's `AstNode<'a>` is arena-allocated, so the real trait either takes a lifetime parameter or, cleaner, renders from an owned intermediate representation converted once from the comrak AST. Decide at M5.)*

One comrak parse → AST → each target is one `Renderer` impl. Targets register in a list; adding a platform later = one new impl. Three delivery tiers:
- **Clipboard (no auth, instant):** LinkedIn (Unicode bold/italic, link-flattening, fold-aware), X thread (semantic split + numbering, code blocks intact), X Article (rich HTML paste).
- **File (download via native save dialog):** clean semantic HTML (syntax highlight + KaTeX), `.docx` (`docx-rs` crate), later PDF/EPUB/RTF.
- **Publish (API, v2):** Ghost (Admin API), beehiiv, Dev.to, Hashnode, WordPress. (Substack/Medium have no usable API → clipboard/paste only.)
- **AI-adapted export (v2 magic):** renderer + AI together → platform-*native* content, not just mechanical conversion.

## Port map (salvage from AgentDocs)

| Carry over | Disposition |
|---|---|
| `client/src/index.css` (~1,680 lines) | **PORTS DIRECTLY** — drop only the `.yRemoteSelection*` cursor block + presence/connection rules. Whole visual identity + light/dark tokens free. |
| CodeMirror 6 config in `Editor.tsx` (themes, highlight tables, compartments, markdown+language-data, updateListener) | **PORTS** — strip the one `yCollab(ytext, awareness)` line; pass initial content via `EditorState.create({doc})`. |
| `utils/formatting.ts` (14 toolbar commands) + `@codemirror/commands` | **PORTS DIRECTLY** — pure CM6 transactions, framework-agnostic. |
| `useMarkdownPreview` (remark + gfm + html, 150ms debounce) | **REPLACED** — keep the 150ms-debounce pattern as a Svelte util, but rendering moves to a `render_preview` Tauri command (comrak). Drops the remark/unified deps; one engine for preview + export (decision 7). |
| `templates/index.ts` (5 types) | **PORTS** — fix `skill` frontmatter to real spec (`name:` + `description:`, drop `triggers:`). |
| `types/documentTypes.ts`, `buildSidebarTree`, `documentIcons` (SVG paths) | **PORTS** — types/logic verbatim; icons rewrap as Svelte. |
| AI prompt-building + chat-accumulation logic (`useAssistant.ts`) | **ADAPTS** — logic kept; HTTP call moves to Rust (see AI layer). |
| All React components (Editor/Preview/Sidebar/Toolbar/etc.) | **ADAPTS** — React→Svelte 5; state hooks → rune stores. Mechanical (hooks were already cleanly isolated). |
| `exportDocument.ts` (`.md` only today) | **EXPAND** — becomes the Rust export engine above. |
| Yjs / y-websocket / `usePresence` / Express server / JSON storage | **DROP** — replaced by SQLite + Tauri commands. |

## Milestones

- [x] **M0 — Scaffold.** Tauri v2 + SvelteKit (adapter-static) skeleton in `~/GitHub/markdown`. Rust deps wired (rusqlite WAL, comrak, reqwest, uuid, chrono, serde). App boots to an empty window. ✅ 2026-06-09
- [x] **M1 — Storage.** SQLite schema + migrations; documents/folders Tauri commands; content load/save. Verified via a smoke test. ✅ 2026-06-09
- [x] **M2 — Editor.** Port CodeMirror 6 config + the CSS design system + toolbar. Live editing persists to SQLite (debounced). ✅ 2026-06-09
- [x] **M3 — Preview + Sidebar.** Live preview (comrak via `render_preview` command, 150ms debounce), sidebar tree, doc/folder CRUD, templates, light/dark. ✅ 2026-06-09
- [x] **M4 — AI assistant.** Rust streaming AI via events; Svelte chat panel; key in keychain; inline "apply to document". ✅ 2026-06-09
- [x] **M5 — Export v1.** Renderer trait + targets: LinkedIn (clipboard), clean HTML (file), `.docx` (file). Native save dialogs. (X thread deferred to v2 per decision 2.) ✅ 2026-06-09
- [x] **M6 — Polish.** Settings, model picker, error states, multi-preview. First usable daily-driver build. ✅ 2026-06-09 — **v1 complete.**

### AI chat backlog (user request, 2026-06-09 — revisit after M5)
- [x] **Multiple chats per document** — shipped 2026-06-10 (v2 roadmap #3). Migration v4 `chats` table + `chat_messages.chat_id`; lazy backfill of pre-v4 threads on first access; chat switcher dropdown + new/delete in the assistant panel; auto-title from the first user message.
- [x] **Token + context visibility** — shipped 2026-06-10. `assistant:usage` event (Anthropic `message_start`/`message_delta` usage; OpenRouter final `usage` chunk via `stream_options.include_usage`); per-message in/out token counts persisted on `chat_messages`; running context size in the panel header.
- [ ] **Context management / compaction** — long threads will blow up cost and context; summarize or truncate older turns (Anthropic has server-side compaction, beta `compact-2026-01-12`; OpenRouter path needs client-side strategy).
- [ ] General hardening pass on the assistant — user expects to do "a lot" here; treat the chat as a core surface, not a bolt-on.

### Export backlog (2026-06-09)
- [x] **X thread + X Article (rich clipboard)** — shipped 2026-06-10 (v2 roadmap #1). `export/x.rs`: thread segmenter (≤280-char numbered posts, code blocks intact, links flattened) + plain renderer; `x-article` uses an HTML-flavored clipboard write (`navigator.clipboard.write` with text/html + text/plain) reusing the preview HTML. New `ExportOutput::ClipboardHtml` variant; `render_x_thread_preview` command + an "X thread" preview pill. 7 unit tests.
- [ ] Docx polish round 2 pending user feedback (fonts/spacing/tables shipped 2026-06-09).
- [ ] **v2 (later):** publish-to-API targets (Ghost/beehiiv/Dev.to), AI-adapted export, optional cloud sync/backup, PDF/EPUB.

## v1 scope line (ship this, defer the rest)
**In:** editor + AI assist + local SQLite + 3 export targets — **LinkedIn (clipboard), clean HTML (file), .docx (file, via docx-rs)**.
**Out (v2):** collaboration (cut for good), API publishing, cloud sync, AI-adapted export, PDF/EPUB, and X-thread export (deferred — trait makes it a quick add later).

## Decisions (RESOLVED 2026-06-08)
1. **Folders:** flat one-level for v1. ✅
2. **v1 export targets:** LinkedIn (clipboard) + clean HTML (file) + .docx (file). X thread deferred to v2. ✅
3. **AI key storage:** OS keychain (`keyring` crate). ✅
4. **`.docx`:** build in v1 with `docx-rs` (AST → docx, not the HTML shortcut). Highest-demand path, worth doing right early. ✅
5. **Agent-file templates:** keep as a light bonus; lead the creator/publication pitch. ✅
6. **Project root:** `~/GitHub/markdown` (current dir). ✅
7. **Preview engine:** comrak in Rust via `render_preview` command — one engine for preview, AI context, and export (no remark/unified in the frontend). The 150ms debounce makes the IPC round trip a non-issue. ✅ (2026-06-09)

## Review section

### M0 — Scaffold (2026-06-09)
- Scaffolded with `create-tauri-app` (svelte-ts template = SvelteKit + adapter-static + Svelte 5), renamed `markdown-scaffold` → `markdown`, identifier `com.adamwickwire.markdown`, window 1280×800.
- Rust deps wired: rusqlite 0.40 (bundled), comrak 0.52 (**default-features off** — defaults pull in a full CLI: clap/syntect/xdg), reqwest 0.13 (rustls, json, stream — note: feature renamed from `rustls-tls` to `rustls` in 0.13), uuid v4, chrono (serde), anyhow, thiserror 2, tokio.
- **Toolchain bump required:** libsqlite3-sys 0.38 uses `cfg_select!` → needs rustc ≥1.95; updated stable 1.94.1 → 1.96.0.
- Stripped template demo (greet command, opener plugin + capability, logo SVGs) → boots to a clean placeholder shell.
- Verified: `pnpm check` 0 errors, `pnpm build` ok, `cargo build` ok, app process launches and runs.
- Git repo initialized; nothing committed yet.

### M1 — Storage (2026-06-09)
- `src-tauri/src/storage.rs`: schema (spec'd `folders` + `documents`), `PRAGMA user_version` append-only migrations, WAL + foreign_keys on, all CRUD + content load/save as plain functions over `&Connection` (testable without Tauri). `DocType` enum with kebab-case serde + `as_str`/`parse` (unknown stored types degrade to `generic` on read).
- `src-tauri/src/commands.rs`: 11 thin Tauri commands over `Db(Mutex<Connection>)` state. `src-tauri/src/error.rs`: thiserror enum, serialized as message string over IPC.
- `lib.rs` setup: opens `app_data_dir()/markdown.db`, runs init, manages state.
- `create_document` also takes optional initial `content` (templates will need it at M3).
- Verified: 10 unit tests green (CRUD, content, FK orphaning on folder delete, NotFound paths, name validation, serde roundtrip); clippy clean; live boot created `markdown.db` with `journal_mode=wal`, `user_version=1`, both tables.

### M2 — Editor (2026-06-09)
- **Note: old AgentDocs moved** — it lives at `~/Desktop/markdown-collab` now, not `~/GitHub/markdown-collab`.
- `src/app.css`: full 1,680-line design system ported (1,604 after stripping `.yRemoteSelection*`, `.sidebar-presence*`, `.topbar-avatar*`/`.topbar-users`). Theme = `data-theme` attr on `<html>`. IBM Plex bundled via @fontsource (was Google Fonts — desktop apps should work offline).
- `src/lib/editor/themes.ts`: CM6 dark/light themes + highlight styles ported from `Editor.tssx`; `formatting.ts` copied verbatim (pure CM6 transactions).
- `src/lib/components/Editor.svelte`: CM6 in `onMount` (content prop is initial-only — a `$effect` would rebuild the editor per keystroke; remount via `{#key doc.id}` to switch docs), theme switch via Compartment reconfigure. `Toolbar.svelte`: all 16 buttons, icon snippet + `{@html}` for static path data.
- `src/lib/api.ts`: typed invoke wrappers (Tauri camelCase→snake_case arg mapping).
- `+page.svelte`: boot = list docs → most-recent or create "Untitled" → load content; 500ms debounced save + flush on close/destroy.
- Verified: `pnpm check` 0 errors, build ok; live run created "Untitled" on first boot; typed text persisted to SQLite (content + fresh `updated_at` confirmed via sqlite3).

### M3 — Preview + Sidebar (2026-06-09)
- `preview.rs`: comrak `render_preview` (GFM: table/strikethrough/tasklist/autolink/footnotes; `front_matter_delimiter="---"` strips YAML; raw HTML escaped — preview pane has IPC access). One options fn shared with future export.
- Ported: Sidebar (folders/CRUD/inline rename), NewDocumentDialog (9 types, creator-first; **Blank first + default per user request**), TopBar (sidebar toggle, theme toggle, **click-to-rename doc title** — user request), Preview, StatusBar (words + Ln/Col), MoveToFolderMenu, DocumentIcon. Templates incl. new creator templates + fixed skill frontmatter.
- `tauri-plugin-dialog` for native confirms (WKWebView `window.confirm` unreliable; needed for M5 save dialogs anyway). Editor: added `EditorView.lineWrapping` (prose app).
- Theme: `data-theme` on `<html>` + CM Compartment swap, persisted in localStorage.
- Verified live by user (screenshots): table/list/heading rendering, frontmatter stripped, wrapping, rename, templates.

### M4 — AI assistant (2026-06-09)
- `ai.rs`: providers **Anthropic** (`/v1/messages`, adaptive thinking, default `claude-opus-4-8`) + **OpenRouter** (`/chat/completions`, default `anthropic/claude-opus-4.8`, verified vs live catalog). reqwest SSE → Tauri events `assistant:token/done/error`; abortable task in `AiState`; per-provider keys.
- **Key storage: debug builds = plain `dev-keys.json` in app data dir** (keychain re-prompts on every dev rebuild — see lessons.md); release = OS keychain. Provider+model (non-secret) in localStorage.
- Frontend: `assistant.svelte.ts` rune store; RightPaneTabs (Preview/Assistant); AssistantPanel with inline provider/model/key setup ("key saved" status), streaming chat, Copy / **Insert (any md block, at cursor)** / Replace document (full rewrites).
- System prompt injects current doc content per message (chat-history + current doc only — no other docs/tools).
- Verified live by user: doc-context Q&A ✓, streaming via OpenRouter ✓, stop ✓, Insert ✓.
- **Post-M4 addition (user request):** per-document persistent chats — migration v2 `chat_messages` table (CASCADE on doc delete), `get/save_chat_messages` commands (replace-all semantics), store loads thread on doc switch, persists on done/error/clear. Verified by user.

### M5 — Export v1 (2026-06-09)
- `export/` module: `mod.rs` (TARGETS const + ExportOutput enum: clipboard/file/cancelled), `linkedin.rs`, `html.rs`, `docx.rs`. All parse via `preview::options()` — one comrak engine everywhere. (No Renderer trait — comrak's arena lifetime made a static-dispatch match on target id simpler; trait can come back if targets multiply.)
- **LinkedIn (clipboard):** Unicode Math Sans-Serif Bold/Italic mapping (incl. bold digits), strikethrough via U+0336 combining, • bullets + numbered lists w/ indent, link flattening "text (url)", headings → bold lines, frontmatter stripped. Frontend copies via navigator.clipboard. **User: "looks fucking amazing… hottest ticket."**
- **HTML (file):** self-contained semantic doc, embedded GitHub-ish typography, dark-mode media query, escaped title.
- **docx (file):** docx-rs structural build — Calibri 11pt defaults, LineSpacing before/after on headings/body (Word adds none), real bordered tables w/ bold header row, mono code lines, indented italic quotes, "•/1." prefixed lists. Round 2 (font/spacing/tables) after user's "styling fucked up" feedback → "perfect".
- Save dialogs: Rust-side `blocking_save_file` in `spawn_blocking`; export flushes pending save first; status message in TopBar.
- Verified: 22 unit tests; live by user (all three targets).

### M6 — Polish (2026-06-09) — v1 COMPLETE
- **Toasts:** `toast.svelte.ts` store + `Toasts.svelte` (bottom-right, dismissible, 6s auto). All page-level async ops wrapped via `run(promise, what)`; failed saves keep `pendingContent` and retry.
- **SettingsDialog:** provider cards + model input w/ datalist suggestions (opus-4-8/sonnet-4-6/haiku-4-5 per provider) + key field with saved-status. Opened from TopBar gear and AssistantPanel (inline key form removed).
- **Editor keymap:** Mod-b/i/k/e → toggleBold/Italic/insertLink/inlineCode (Prec.high over basicSetup).
- **Multi-preview:** Rendered | LinkedIn pills in preview pane; `render_linkedin_preview` command shows exact clipboard text.
- Verified live by user: "everything works."
