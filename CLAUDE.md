# Plume — write once, publish everywhere

Local-first Tauri v2 desktop markdown writing app with an AI writing partner
and per-platform export. Audience: content creators who write in markdown and
publish to many places (blogs, newsletters, LinkedIn, X).

**Direction:** Plume has pivoted toward a "build in public" writing workspace
(the publishing pipeline was cut). Internal planning/strategy notes (product
direction, spec + milestone history, correction rules) are kept locally and
are **not** part of the public repo — this file plus the code are the source
of truth for contributors.

## Stack

- **Shell:** Tauri v2 — Rust backend, SvelteKit webview
- **Frontend:** SvelteKit + adapter-static (SSR off), Svelte 5 runes,
  TypeScript, CodeMirror 6
- **Backend:** rusqlite (bundled, WAL), comrak (default-features off),
  reqwest (rustls), uuid v4, chrono, anyhow, thiserror 2, keyring, docx-rs,
  image + base64 (docx image embedding); `zip` is a dev-dep (export tests
  inflate the produced .docx and assert the OOXML)
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
                chats/chat_messages/snapshots; DocType enum, title_explicit +
                folders.active flags, sort_order (manual reorder), chat_messages
                .raw_content (compaction content-blocks, migration v9)
  commands.rs   thin #[tauri::command] wrappers; Db(Mutex<Connection>) state
  preview.rs    comrak options + render_html (frontmatter stripped, raw HTML
                escaped — preview pane has IPC access, keep it escaped)
  ai.rs         providers (Anthropic /v1/messages + OpenRouter
                /chat/completions), reqwest SSE → events assistant:token/
                content/usage/done/error, abortable task in AiState, key
                storage. Four stream entry points (chat, inline edit, idea
                expand, content multiply), each with its own system prompt;
                voice_section() injects the global Voice & tone into all.
                Anthropic server-side compaction (beta compact-2026-01-12) on
                the chat path; system-prompt caching; token usage parsed from SSE
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
  lib/buildSidebarTree.ts   folder/doc/Inbox tree assembly for the sidebar
                        (each section sorted by sort_order)
  lib/documentTypes.ts  per-type metadata (icon, label) — add a type here
  lib/templates.ts      starter bodies per document type
  lib/editor/           CodeMirror setup: formatting.ts (toolbar commands),
                        themes.ts (light/dark)
  lib/toast.svelte.ts   error toasts — wrap new async UI ops in run(p, what)
  lib/components/       Svelte 5 components (props via $props, runes only).
                        Right pane is tabbed via RightPaneTabs.svelte:
                        Preview / Assistant / History (HistoryPanel = snapshot
                        restore). IdeaCaptureModal = quick-capture; MultiplyModal
                        = target picker. HomeShelf = the project-shelf home shown
                        when no doc is open (the shelf IS the nav — sidebar hides).
  routes/+page.svelte   app shell: doc state, debounced save (500ms) +
                        preview (150ms), export, settings, right-pane tab
```

## Conventions & invariants

- **DB migrations are append-only** — never edit a shipped entry in
  `storage.rs::MIGRATIONS`; add a new one.
- Document `type` is a Rust enum (kebab-case serde); adding a type = one
  variant + frontend `documentTypes.ts` entry, no migration.
- **Ideas are notes, not editor docs.** The `idea` DocType is captured and
  edited only through `IdeaCaptureModal` (never the main editor); it lives in
  the Inbox, and is promoted to a real doc via `update_document_type`. See the
  [[ideas-are-notes]] memory.
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

- `cargo test --manifest-path src-tauri/Cargo.toml` — storage/preview/export
  unit tests (in-memory SQLite; no Tauri needed; docx tests inflate the output
  zip and assert the OOXML). 90+ tests, keep them green.
- `pnpm check` — svelte-check must stay at 0 errors/0 warnings.
- `pnpm tauri dev` for live verification; SQLite lives at
  `~/Library/Application Support/com.adamwickwire.markdown/markdown.db`
  (inspect with sqlite3 to verify persistence).
- rustc ≥ 1.95 required (libsqlite3-sys uses `cfg_select!`).
