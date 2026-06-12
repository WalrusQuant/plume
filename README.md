# Plume

**Write once, publish everywhere.** A local-first desktop markdown writing
app with an AI writing partner that exports your work — adapted per platform —
to everywhere you publish.

Built for content creators who write in markdown and publish to many places:
blogs, newsletters, LinkedIn, X.

![Plume — the markdown editor with live preview, document shelf, and AI writing partner](docs/screenshot.png)

## Features

- **Markdown editor** — CodeMirror 6 with syntax highlighting, formatting
  toolbar, keyboard shortcuts (⌘B/I/K/E), and line wrapping built for prose
- **Live preview** — GitHub-flavored markdown (tables, task lists,
  footnotes) rendered by the same engine that drives every export, with a
  per-platform preview (LinkedIn, X thread, X Article) that shows exactly
  what the clipboard export will paste
- **AI writing partner** — streaming chat with your document as context;
  **multiple conversations per document** with input/output token usage shown;
  threads persist across restarts; insert suggestions at the cursor or replace
  the whole document with one click. Works with **Anthropic** or **OpenRouter**
  (any model they serve)
- **Inline AI edit** — select text, get a streamed rewrite previewed in place,
  then accept or reject — no copy-paste round trip through the chat
- **Voice & tone** — describe how your writing should sound once in settings;
  it's injected into every AI request (chat, inline edit, idea expansion) so
  generated text sounds like you
- **Content multiplication** — from a finished piece, generate platform-native
  variants (blog post, newsletter, LinkedIn post, X thread) in your own voice,
  each a linked, editable document — write once, adapt everywhere
- **Cross-document search + @-mention** — full-text search across everything
  you've written (SQLite FTS5), and @-mention past docs to pull them into the
  chat as context
- **Idea inbox** — capture a half-formed idea in a quick modal without leaving
  what you're writing, optionally let AI expand it into a draft, then convert
  it into a real document when you're ready
- **Version history** — automatic document snapshots you can browse and restore
- **Export anywhere**
  - **LinkedIn** — clipboard-ready text with Unicode bold/italic, real
    bullets, and flattened links (formatting survives the paste)
  - **X (Twitter)** — *thread* mode segments the doc into numbered ≤280-char
    posts (code blocks intact, links flattened); *Article* mode is a rich HTML
    paste matching what the X Article composer keeps
  - **HTML** — clean, self-contained semantic document with dark-mode support
  - **Word (.docx)** — real document structure: built-in heading styles
    (outline/TOC), Word-native numbered/bulleted lists, task-list checkboxes,
    real hyperlinks, embedded images, footnotes, and tables with column
    alignment + header shading
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

v2 (shipped): X thread + X Article export, multiple chats per document with
token usage, version history + restore, inline AI edit, idea inbox, global
Voice & tone, content multiplication, cross-document search + @-mention, the
project shelf home screen, server-side context compaction for long chats, and
a full docx export pass.

Direction: Plume is evolving from a pure distribution tool into a markdown
workspace for **building in public** — plan a project, keep its build log, and
turn that real work into quality posts in your voice. Output stays copy/paste +
export; there is no publishing pipeline.
