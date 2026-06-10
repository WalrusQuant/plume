# Markdown — write once, publish everywhere

Local-first Tauri v2 desktop markdown writing app with an AI writing partner
and per-platform export. Audience: content creators who write in markdown and
publish to many places (blogs, newsletters, LinkedIn, X).

**Read first:** `tasks/todo.md` — full spec, milestone history (v1 = M0–M6,
all complete), resolved decisions, and the active backlogs. `tasks/lessons.md`
holds correction rules; review before making changes.

## Stack

- **Shell:** Tauri v2 — Rust backend, SvelteKit webview
- **Frontend:** SvelteKit + adapter-static (SSR off), Svelte 5 runes,
  TypeScript, CodeMirror 6
- **Backend:** rusqlite (bundled, WAL), comrak (default-features off),
  reqwest (rustls), uuid v4, chrono, anyhow, thiserror 2, keyring, docx-rs
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
                doc/folder/chat CRUD as plain fns over &Connection (testable)
  commands.rs   thin #[tauri::command] wrappers; Db(Mutex<Connection>) state
  preview.rs    comrak options + render_html (frontmatter stripped, raw HTML
                escaped — preview pane has IPC access, keep it escaped)
  ai.rs         providers (Anthropic /v1/messages + OpenRouter
                /chat/completions), reqwest SSE → events assistant:token/
                done/error, abortable task in AiState, key storage
  export/       linkedin.rs (Unicode clipboard text), html.rs (self-contained
                doc), docx.rs (structural docx-rs build)
  error.rs      thiserror enum, serialized as message string over IPC

src/
  lib/api.ts            typed invoke() wrappers — keep 1:1 with commands
  lib/assistant.svelte.ts  chat rune store (per-doc threads, persisted)
  lib/toast.svelte.ts   error toasts — wrap new async UI ops in run(p, what)
  lib/components/       Svelte 5 components (props via $props, runes only)
  routes/+page.svelte   app shell: doc state, debounced save (500ms) +
                        preview (150ms), export, settings
```

## Conventions & invariants

- **DB migrations are append-only** — never edit a shipped entry in
  `storage.rs::MIGRATIONS`; add a new one.
- Document `type` is a Rust enum (kebab-case serde); adding a type = one
  variant + frontend `documentTypes.ts` entry, no migration.
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
  unit tests (in-memory SQLite; no Tauri needed). 22+ tests, keep them green.
- `pnpm check` — svelte-check must stay at 0 errors/0 warnings.
- `pnpm tauri dev` for live verification; SQLite lives at
  `~/Library/Application Support/com.adamwickwire.markdown/markdown.db`
  (inspect with sqlite3 to verify persistence).
- rustc ≥ 1.95 required (libsqlite3-sys uses `cfg_select!`).
