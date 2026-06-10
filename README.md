# Markdown

**Write once, publish everywhere.** A local-first desktop markdown writing
app with an AI writing partner that exports your work — adapted per platform —
to everywhere you publish.

Built for content creators who write in markdown and publish to many places:
blogs, newsletters, LinkedIn, X.

## Features

- **Markdown editor** — CodeMirror 6 with syntax highlighting, formatting
  toolbar, keyboard shortcuts (⌘B/I/K/E), and line wrapping built for prose
- **Live preview** — GitHub-flavored markdown (tables, task lists,
  footnotes) rendered by the same engine that drives every export, plus a
  **LinkedIn mode** that shows exactly what the clipboard export will paste
- **AI writing partner** — streaming chat with your document as context;
  per-document conversations persist across restarts; insert suggestions at
  the cursor or replace the whole document with one click. Works with
  **Anthropic** or **OpenRouter** (any model they serve)
- **Export anywhere**
  - **LinkedIn** — clipboard-ready text with Unicode bold/italic, real
    bullets, and flattened links (formatting survives the paste)
  - **HTML** — clean, self-contained semantic document with dark-mode support
  - **Word (.docx)** — real document structure: heading styles, tables,
    lists, code blocks
- **Templates** — blog post, newsletter, LinkedIn post, X thread, plus
  agent-file types (Claude Code skills, CLAUDE.md, system prompts, runbooks)
- **Local-first** — everything lives in a SQLite database on your machine.
  API keys are stored in the macOS Keychain and never touch the UI layer.
  The only data that leaves your machine is what you send to your chosen
  AI provider.
- Light/dark themes, folders, full-document autosave

## Stack

| Layer | Tech |
|---|---|
| Shell | Tauri v2 (Rust) |
| Frontend | SvelteKit (static) · Svelte 5 · TypeScript · CodeMirror 6 |
| Markdown engine | comrak (one parse feeds preview, AI context, and exports) |
| Storage | SQLite via rusqlite (WAL) |
| AI | Anthropic Messages API / OpenRouter, streamed via SSE from Rust |
| Documents | docx-rs for Word export |

## Development

Prereqs: Rust ≥ 1.95, Node 22+, pnpm.

```sh
pnpm install
pnpm tauri dev      # run the app with hot reload
```

Verification:

```sh
pnpm check                                        # svelte-check
cargo test --manifest-path src-tauri/Cargo.toml   # Rust unit tests
pnpm tauri build                                  # production bundle
```

Notes for development builds:

- AI keys are stored in a plain `dev-keys.json` in the app data folder
  instead of the Keychain (rebuilds change the binary signature, which would
  otherwise trigger endless password prompts). Release builds use the
  Keychain.
- The database lives at
  `~/Library/Application Support/com.adamwickwire.markdown/markdown.db`.

## Project docs

- `tasks/todo.md` — spec, milestone history, decisions, and backlogs
- `CLAUDE.md` — architecture map and conventions for AI-assisted development

## Roadmap

v1 (complete): editor + AI assistant + LinkedIn/HTML/docx export.

Next up: X Article rich-paste export, multiple chats per document,
token/context visibility, compaction. Later: direct API publishing
(Ghost, beehiiv, Dev.to), AI-adapted per-platform export, PDF/EPUB.
