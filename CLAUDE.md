# Plume — a local-first AI writing studio

Local-first Tauri v2 desktop markdown app. Write in markdown with an AI partner
that knows your work: it **semantically searches everything you've written**
(on-device embeddings), works with **reference documents you import** (Markdown /
text / PDF / Word), helps you draft and edit, and adapts a finished piece for
per-platform export. Audience: people who write in markdown and build in public.

**Direction:** a "build in public" writing workspace (the publishing pipeline was
cut — output is copy/paste + export, no auto-post). Internal planning/strategy
notes (product direction, spec + milestone history, correction rules) are kept
locally and are **not** part of the public repo — this file plus the code are the
source of truth for contributors.

## Stack

- **Shell:** Tauri v2 — Rust backend, SvelteKit webview
- **Frontend:** SvelteKit + adapter-static (SSR off), Svelte 5 runes,
  TypeScript, CodeMirror 6
- **Backend:** rusqlite (bundled, WAL), comrak (default-features off),
  reqwest (rustls), uuid v4, chrono, anyhow, thiserror 2, keyring, docx-rs,
  image + base64 (docx image embedding); **fastembed** (on-device embeddings,
  `ort` static-linked, ort-download-binaries) for the semantic notebook;
  **pdf-extract** (PDF import), **quick-xml** + **zip** (DOCX import — `zip` is
  also used by export tests to inflate the produced .docx and assert the OOXML)
- **Styling:** CSS custom properties design system in `src/app.css`
  (`data-theme` attr on `<html>`, light/dark) — **no Tailwind**
- **Build:** pnpm + Vite; `pnpm tauri dev` to run

## Architecture (one engine, three uses)

One comrak parse (options in `src-tauri/src/preview.rs::options()`) feeds:
(a) live preview, (b) AI context, (c) every export renderer. Never add a
second markdown engine.

```
src-tauri/src/
  lib.rs        Tauri setup: DB at app_data_dir/markdown.db, command registry
  storage.rs    schema + PRAGMA user_version migrations (APPEND-ONLY list),
                CRUD as plain fns over &Connection (testable) for docs/folders/
                chats/chat_messages/snapshots; DocType enum (incl. idea, source),
                title_explicit + folders.active flags, sort_order (manual
                reorder), chat_messages.raw_content (compaction blocks, v9),
                chunks table + documents.embedded_at (semantic index, v10),
                app_settings KV (v11, e.g. active embed model);
                get/set_setting, all_chunk_embeddings, docs_needing_embedding,
                replace_chunks, clear_index_for_reembed
  commands.rs   thin #[tauri::command] wrappers; Db(Mutex<Connection>) state.
                Also embed-model cmds (status/download/remove/list/get/set) and
                import_documents (extract → create doc → nudge embed worker)
  preview.rs    comrak options + render_html (frontmatter stripped, raw HTML
                escaped — preview pane has IPC access, keep it escaped)
  ai.rs         providers (Anthropic /v1/messages + OpenRouter
                /chat/completions), reqwest SSE → events assistant:token/
                content/usage/done/error, abortable task in AiState, key
                storage. Four stream entry points (chat, inline edit, idea
                expand, content multiply), each with its own system prompt;
                voice_section() injects the global Voice & tone into all.
                Anthropic server-side compaction (beta compact-2026-01-12) on
                the chat path; system-prompt caching; token usage parsed from SSE.
                Chat is an agentic tool loop (both providers) with two tools:
                web_search (Tavily, via websearch.rs) and search_notes (semantic
                RAG over the user's docs). run_semantic_search guards on model
                installed + dim-matches the query; both default OFF per chat.
  embed.rs      semantic notebook: chunk_document (block-boundary chunking,
                capped at CHUNK_MAX_WORDS so nothing overruns the ~512-token
                model window), the Embedder trait + fastembed-backed
                FastEmbedder (CURATED_MODELS catalog, per-model dim/prefixes,
                opt-in download only via ensure_loaded), embedding BLOB (de)ser,
                and the background indexing worker (own DB conn, never the Db
                mutex; nudged by writes)
  import.rs     extract_text(path) by extension — md/txt verbatim, PDF via
                pdf-extract (catch_unwind-guarded), DOCX via zip + quick-xml
                <w:t> runs. Best-effort/lossy; empty output = a failed import
  websearch.rs  Tavily client (BYOK) for the web_search tool
  export/       one renderer per publish target, all fed by the same comrak
                parse. mod.rs holds the TARGETS list + ExportOutput enum
                (Clipboard plain / ClipboardHtml rich-paste / File / Cancelled).
                Targets: linkedin.rs (Unicode-styled clipboard), x.rs (X thread
                ≤280 + X-Article rich HTML), mastodon.rs / bluesky.rs /
                threads.rs (char-limited threads — all delegate to social.rs),
                reddit.rs / discord.rs (markdown re-serialization, footnotes
                stripped, tables re-emitted/flattened), telegram.rs (Telegram
                HTML subset), richhtml.rs (generic rich paste — Google Docs +
                Newsletter flavors), html.rs (standalone .html), docx.rs (full
                docx-rs build), rtf.rs (hand-rolled RTF), markdown.rs (raw
                source .md), plaintext.rs (stripped .txt). social.rs is the
                shared thread segmenter used by every char-limited social target
  error.rs      thiserror enum, serialized as message string over IPC

src/
  lib/api.ts            typed invoke() wrappers — keep 1:1 with commands
  lib/assistant.svelte.ts  chat rune store (multi-thread per doc, token usage,
                        persisted settings incl. Voice & tone)
  lib/inlineEdit.svelte.ts  selection-menu AI edit: streamed preview, accept/
                        reject against the CodeMirror selection
  lib/ideaExpand.svelte.ts  idea-inbox expansion stream controller
  lib/multiply.svelte.ts    content-multiplication controller (source doc →
                        per-target platform variants, sequential streams)
  lib/buildSidebarTree.ts   folder/doc/Ideas/Sources tree assembly for the
                        sidebar (each section sorted by sort_order; ideas AND
                        sources are split out of the editable doc tree)
  lib/documentTypes.ts  per-type metadata (label) + the DocType exhaustiveness
                        guard — register/exclude a new type here
  lib/templates.ts      starter bodies per document type (Record<DocType>)
  lib/editor/           CodeMirror setup: formatting.ts (toolbar commands),
                        themes.ts (light/dark)
  lib/toast.svelte.ts   error toasts — wrap new async UI ops in run(p, what)
  lib/components/       Svelte 5 components (props via $props, runes only).
                        Right pane is tabbed via RightPaneTabs.svelte:
                        Assistant (default) / Preview / History / Guide.
                        HistoryPanel = snapshot restore; IdeaCaptureModal =
                        quick-capture; MultiplyModal = target picker;
                        ImportModal = documents-vs-sources picker on import;
                        SourceViewerModal = read-only source view (+ remove /
                        convert-to-doc); SettingsDialog is tabbed (AI | Local
                        search — the embed-model download/remove + picker).
                        HomeShelf = the project-shelf home when no doc is open
                        (the shelf IS the nav — sidebar hides).
  routes/+page.svelte   app shell: doc state, debounced save (500ms) +
                        preview (150ms), export, settings, right-pane tab
```

## Conventions & invariants

- **DB migrations are append-only** — never edit a shipped entry in
  `storage.rs::MIGRATIONS`; add a new one.
- Document `type` is a Rust enum (kebab-case serde); adding a type = one Rust
  variant (enum + `as_str`/`parse`) + these frontend `Record<DocType>` sites or
  it won't compile: `api.ts` union, `documentTypes.ts` (`DOCUMENT_TYPES` and the
  `_EXHAUSTIVE` guard), `templates.ts`, `DocumentIcon.svelte`. No migration.
- **Ideas are notes, not editor docs.** The `idea` DocType is captured and
  edited only through `IdeaCaptureModal` (never the main editor); it lives in
  the Inbox, and is promoted to a real doc via `update_document_type`. See the
  [[ideas-are-notes]] memory.
- **Sources are read-only reference docs.** The `source` DocType is created only
  by import (`import_documents` with `as_source`); it lives in the sidebar
  "Sources" section, opens in `SourceViewerModal` (never the editor), and is
  removed by delete (chunks cascade out of the index) or promoted via
  `update_document_type`. Sources ARE embedded/searchable (non-idea). Keep them
  out of the editable tree everywhere (buildSidebarTree, HomeShelf recent,
  search-result click routing). See [[document-import-parked]].
- **Semantic notebook is opt-in and single-model.** The embedding model is only
  ever downloaded by the explicit Settings button (`ensure_loaded`) — the worker
  and chat path never auto-download; they no-op if `!is_installed()`. Vectors
  across models/dims aren't comparable, so switching the active model wipes +
  re-embeds the whole index (`clear_index_for_reembed`); default stays bge-small
  (384) so existing indexes remain valid. `chunk_document` caps chunks at
  `CHUNK_MAX_WORDS` because models truncate past ~512 tokens. See
  [[embed-model-opt-in]].
- `title_explicit` (documents column) distinguishes a user-set title from a
  derived one (ideas default their name to the first line). The capture/rename
  paths set it; don't overwrite an explicit title with a derived one.
- **Voice & tone is global**, stored in the persisted assistant settings and
  passed as the `voice` arg to every AI stream command. `ai.rs::voice_section`
  appends it to all three system prompts; it must never override mechanical
  rules (e.g. inline edit's "return only the replacement text").
- Snapshots are append-only history rows (`SnapshotCause` enum). History is
  restore-only — never mutate or delete past snapshots.
- **Export gaps are fixed in the renderer, never by toggling the shared
  `preview::options()`** — every target parses the same comrak AST, so a
  docx/x/linkedin fix that changes parse options would alter the live preview
  too. Add a `NodeValue` arm in the renderer instead.
- **AI keys never touch the webview.** Release builds use the OS keychain;
  debug builds use `dev-keys.json` in app data dir (keychain re-prompts on
  every dev rebuild — do not "fix" this back to keychain).
- AI provider/model defaults live in `ai.rs::Provider::default_model()`.
  Verify current model IDs against the claude-api skill / OpenRouter catalog
  before changing — do not guess from memory.
- Editor content flows one way: CodeMirror owns the text after mount
  (`content` prop is initial-only; remount via `{#key doc.id}` to switch
  docs). Programmatic edits go through `editorView.dispatch` so the
  updateListener triggers save + preview.
- Frontend↔Rust arg names: Rust snake_case params are called with camelCase
  keys from JS (Tauri converts).
- New user-facing failures: surface via `toast.error`, not console.

## Testing / verification

- `cargo test --manifest-path src-tauri/Cargo.toml` — storage/preview/export/
  embed/import unit tests (in-memory SQLite; no Tauri needed; docx tests inflate
  the output zip and assert the OOXML; embed/import tests use temp dirs). 150+
  tests, keep them green.
- `pnpm check` — svelte-check must stay at 0 errors/0 warnings.
- `pnpm tauri dev` for live verification; SQLite lives at
  `~/Library/Application Support/com.adamwickwire.markdown/markdown.db`
  (inspect with sqlite3 to verify persistence).
- rustc ≥ 1.95 required (libsqlite3-sys uses `cfg_select!`).
