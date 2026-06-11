# v2 Roadmap — from markdown editor to content operation

> Status: **PLANNING ONLY — nothing approved for build.** Nine features, ordered
> easiest → most complex. Each entry: what it is, how to implement it against
> the current codebase, effort, dependencies, and open decisions.
>
> Theme: v1 helps you *write* a document. v2 helps you run a *content
> operation* — multiply one piece across platforms, in your own voice, and
> track it from idea to published. Features 1–7 are independent bricks;
> 8 and 9 are the payoff that assembles them.

## Order at a glance

| # | Feature | Complexity | Schema change | Depends on |
|---|---------|-----------|---------------|------------|
| 1 | X thread + X Article export | Low | none | — | ✅ done 2026-06-10 |
| 2 | Document snapshots / history | Low | migration v3 | — | ✅ done 2026-06-10 |
| 3 | Multiple chats per doc + token visibility | Low-Med | migration v4 | — | ✅ done 2026-06-10 |
| 4 | Inline AI edit (selection menu) | Medium | none | #2 strongly recommended | ✅ done 2026-06-10 (selection menu, click-driven) |
| 5 | Idea inbox / quick capture | Medium | migration v5 | — | ✅ done 2026-06-10 (click-driven Inbox; redesigned to notes-in-a-modal, PR #6) |
| 6 | Voice profile | Medium | none | — | ✅ done 2026-06-10 (shipped as a global Voice & tone setting injected into all prompts; exemplar-picker + AI distillation deferred) |
| 7 | Cross-document memory (search + AI recall) | Med-High | FTS5 migration | — |
| 8 | Pipeline & publishing | High | migration(s) | — |
| 9 | Content multiplication | High | small migration | #6, ideally #7 + #8 |

Rationale for the order: 1–3 are contained wins (one renderer, one table, one
refactor) that build muscle for the bigger ones. 4 is the first "wow" feature
and needs 2 as its safety net. 5–7 are independent mid-size features that can
be reshuffled freely. 8 and 9 are large and benefit from everything before
them — and 9 is the moat, so everything funnels toward it.

---

## 1. X thread + X Article export

**What:** Two new export targets. *X thread*: split the doc into numbered
posts (semantic split at paragraph/heading boundaries, ~270 chars each,
code blocks kept intact, `1/n` numbering). *X Article*: rich HTML clipboard
write so a paste into the X article composer keeps formatting (same trick
that made the LinkedIn export the "hottest ticket").

**Why:** Cheapest extension of the proven winner; feeds the
"publish everywhere" pitch with a second platform.

**Implementation:**
- `src-tauri/src/export/x_thread.rs` — parse via `preview::options()` (one
  engine, decision 7). Walk the AST: accumulate blocks into posts ≤ ~270
  chars (leave headroom for `🧵 1/n`), never split inside a code block or
  list item; flatten links LinkedIn-style (`text (url)`). Output
  `Vec<String>`; clipboard delivery joins with `\n\n---\n\n` (v1) — a
  per-post copy UI can come later.
- *X Article* needs an **HTML-flavored clipboard write**. `navigator.clipboard.write`
  with a `text/html` ClipboardItem may be enough in WKWebView; if not, add
  `tauri-plugin-clipboard-manager` (supports HTML) and a new `ExportOutput`
  variant `ClipboardHtml { html: String, plain: String }`. Renderer reuses
  `export/html.rs` body rendering minus the document chrome.
- Register both in `export::TARGETS`; one new match arm each in
  `commands.rs::export_document`. Frontend export menu picks them up from
  `list_export_targets()` automatically.
- Tests mirror `linkedin.rs`'s: split boundaries, code-block integrity,
  numbering, link flattening.

**Effort:** ~1 day. **Decisions:** exact char budget per post (270 vs 280 −
numbering); whether thread export gets its own preview mode pill next to
Rendered | LinkedIn (recommended: yes, it's ~30 lines given the pattern).

---

## 2. Document snapshots / history

**What:** Automatic point-in-time copies of a document's content: before
every AI-applied edit (manual "restore point" too), and at most one ambient
snapshot per N minutes of active editing. A history panel lists snapshots;
restore replaces the doc (after snapshotting the current state).

**Why:** Session undo dies with the window. Once AI can rewrite text in
place (#4), pre-edit snapshots stop being nice-to-have and become the
safety story that makes users trust the feature.

**Implementation:**
- Migration v3 (append to `storage.rs::MIGRATIONS`):
  ```sql
  CREATE TABLE snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    cause TEXT NOT NULL,          -- 'ai-edit' | 'interval' | 'manual' | 'restore'
    created_at TEXT NOT NULL
  );
  CREATE INDEX idx_snapshots_doc ON snapshots(document_id, created_at DESC);
  ```
- `storage.rs`: `create_snapshot(conn, doc_id, content, cause)` (skip if
  content identical to the latest snapshot), `list_snapshots(conn, doc_id)`
  (id/cause/created_at/word-count — not full content), `get_snapshot_content`,
  and pruning: keep all from the last 24 h, then thin to one/day, cap ~50
  per doc. Prune inside `create_snapshot` so there's no background job.
- Commands + `api.ts` wrappers (keep 1:1). Interval snapshots are
  frontend-driven: in `+page.svelte`'s save path, if >N min since last
  snapshot for this doc, call `create_snapshot` before `save_document_content`.
- UI: "History" entry (TopBar overflow or StatusBar), right-pane list with
  relative times + cause badges, Preview/Restore buttons. Restore goes
  through `editorView.dispatch` (invariant: editor owns the text).

**Effort:** 1–2 days. **Decisions:** interval length (suggest 10 min);
whether restore needs a confirm dialog (suggest yes, via the existing
`tauri-plugin-dialog` confirm).

---

## 3. Multiple chats per document + token visibility

**What:** The backlogged items from todo.md. New-chat button, a thread
switcher per doc, and per-message token usage / running context size.

**Why:** One eternal thread per doc gets noisy and silently expensive.
This is also the schema groundwork every later AI feature builds on.

**Implementation:**
- Migration v4:
  ```sql
  CREATE TABLE chats (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    title TEXT NOT NULL DEFAULT 'New chat',
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
  );
  ALTER TABLE chat_messages ADD COLUMN chat_id TEXT REFERENCES chats(id) ON DELETE CASCADE;
  -- data migration: one chat row per document that has messages; backfill chat_id
  ```
  Append-only as always; keep `document_id` on `chat_messages` readable until
  a later cleanup migration drops reliance on it.
- `storage.rs`: `list_chats(doc_id)`, `create_chat(doc_id)`, `rename_chat`,
  `delete_chat`; `get/save_chat_messages` keyed by `chat_id` (keep
  replace-all semantics).
- `assistant.svelte.ts`: store gains `chats: Chat[]` and `activeChatId`;
  `loadFor(docId)` loads the chat list and opens the most recent (creating
  one if none). The stream-id event tagging added in the v1 fixes already
  protects against cross-thread token bleed — no new race handling needed.
- Token usage: providers report it in-stream (Anthropic `message_delta.usage`,
  OpenRouter final-chunk `usage`). In `ai.rs::stream_sse`, let the per-provider
  `extract` also surface usage; emit one `assistant:usage` event
  (`{id, input_tokens, output_tokens}`) before done. Persist per message
  (nullable columns on `chat_messages`); show small counts under assistant
  messages + a running context estimate in the panel header.
  **Verify the exact usage-field shapes against the claude-api skill /
  OpenRouter docs at build time — do not code them from memory.**
- UI: chat title dropdown + "+" in the AssistantPanel header; auto-title
  from the first user message (first ~40 chars; AI titling later).

**Effort:** 2 days. **Decisions:** context *estimate* method client-side
(chars/4 is fine v1); defer compaction (todo.md backlog) until usage data
shows it's needed.

---

## 4. Inline AI edit (selection menu)

**What:** Select text in the editor → floating menu (Rewrite, Expand,
Shorten, Fix grammar/tone, custom instruction) → streamed replacement shown
as a preview → Accept / Reject. The headline feature of v2's first release.

**Why:** Turns the assistant from a side panel into a writing partner — the
product's stated build lead.

**Implementation:**
- **Backend:** new command `inline_edit` reusing the existing plumbing:
  `ai.rs` gains a second prompt builder (system: "You are editing a fragment
  of a markdown document. Return ONLY the replacement text, no preamble, no
  code fences." + doc content for context + the selected fragment + the
  instruction). Reuse `stream_sse` and the same tagged `assistant:*` events —
  the stream-id mechanism already lets a different listener own a stream.
  Use the **fast model** (Haiku tier) by default with a settings override;
  add `Provider::fast_model()` next to `default_model()` (verify IDs against
  the claude-api skill at build time, per CLAUDE.md).
- **Frontend (the real work):**
  - `src/lib/editor/inlineEdit.ts` — a CM6 extension: on non-empty selection,
    show a tooltip (`showTooltip` / `EditorView.updateListener`) with the
    action buttons; Esc dismisses.
  - During streaming, don't mutate the doc. Hold the selection range with
    CM6 decorations: strike/dim the original, render the incoming text in a
    widget below it (a `Decoration.widget` block). On **Accept**: snapshot
    (#2, cause `ai-edit`), then one `editorView.dispatch` replacing the
    range (updateListener handles save/preview per the one-way-flow
    invariant). On **Reject**: drop decorations, untouched doc.
  - Guard rails: lock the selection range while streaming (read-only via
    `EditorState.readOnly` compartment is simplest v1); only one inline edit
    at a time; reuse `toast.error` on stream errors.
  - New rune store `inlineEdit.svelte.ts` (mirrors `assistant.svelte.ts`:
    listens for its own stream id, accumulates text) so AssistantPanel and
    inline edit never fight over events.
- Keyboard entry point: `Mod-j` with a selection opens the menu (matches
  the existing `Prec.high` keymap pattern in `Editor.svelte`).

**Effort:** 3–5 days (CM6 decoration/tooltip work dominates). **Depends:**
#2 for pre-edit snapshots (build #2 first). **Decisions:** diff-style
preview (red/green) vs simple replace preview — start simple; menu action
list v1 (suggest: Rewrite, Shorten, Expand, Custom…).

---

## 5. Idea inbox / quick capture

**What:** A frictionless capture surface: global OS shortcut opens a tiny
always-on-top window (or focuses the app) with one text box; the note lands
in an Inbox section of the sidebar. Later, "Expand with AI" turns a fragment
into a draft outline/post in one click.

**Why:** Creators die at the blank page. This feeds the pipeline's front
end and creates a daily-open habit even when not writing.

**Implementation:**
- Modeling: ideas are just documents — add `DocType::Idea` (one enum variant
  + one `documentTypes.ts` entry, no migration, per the existing convention)
  and pin an "Inbox" virtual section at the top of the sidebar that lists
  `type = 'idea'` docs (no folder machinery).
- Global shortcut: `tauri-plugin-global-shortcut`. v1: shortcut focuses the
  main window and opens a capture modal (one textarea, Enter = save as idea
  doc, Esc = dismiss). A separate mini always-on-top capture window
  (second Tauri WebviewWindow) is a v1.5 polish step — windowing edge cases
  (focus stealing, multi-monitor) make it the riskiest part, so don't start
  there.
- "Expand with AI": one command reusing the chat plumbing with a dedicated
  prompt ("turn this fragment into a structured outline for a {type}");
  output becomes a new doc via the existing `create_document(content)` path,
  idea doc gets linked (see #9's `derived_from` column — or just delete the
  idea on promote, simpler).

**Effort:** 2–3 days (modal version). **Decisions:** default shortcut
(suggest `Cmd/Ctrl+Shift+Space`); whether promote deletes or keeps the idea.

---

## 6. Voice profile

> ✅ **Shipped 2026-06-10 (simplified).** Implemented as a single global **Voice &
> tone** free-text field in Settings, injected (via a shared `voice_section`
> helper in `ai.rs`) after the mechanical rules of all three system prompts —
> chat, inline edit, expand. Rides the existing localStorage AI-settings blob +
> per-request arg plumbing; no migration. Also tightened the three prompts and
> unified the brand to "Plume". **Decision:** product owns the (non-editable)
> system prompts; the user owns only their voice. **Deferred:** the exemplar
> picker + AI distillation below (the voice card can later auto-fill this field).

**What:** The AI writes like *the user*. A settings section where they pick
3–10 of their own pieces as exemplars (or paste samples); the app distills a
style guide ("voice card") that is injected into every generation prompt —
chat, inline edit, and (later) multiplication. Local-first: nothing leaves
the machine except inside prompts they trigger.

**Why:** The #1 complaint about AI writing is that it sounds like AI.
Cheap to build (prompt assembly, not training) and a genuine moat — the
user's corpus is already in our SQLite.

**Implementation:**
- Distillation: command `build_voice_profile(doc_ids)` — concatenates the
  exemplars (frontmatter stripped via the one comrak engine), sends one
  strong-model request: "produce a compact style guide: tone, sentence
  rhythm, vocabulary quirks, formatting habits, things this writer never
  does; ≤400 words." Store the result + the exemplar doc ids + a
  user-editable version of the text.
- Storage: single-row settings table (migration), or a `settings(key, value)`
  KV table — prefer the KV table, #8 and #9 will want it too.
- Injection: `ai.rs::system_prompt` gains an optional voice section
  (`"Write in the user's voice:\n{voice_card}"`). Chat and inline edit both
  pass it; per-request "ignore voice" toggle in the UI.
- UI: Settings dialog gains a "Voice" tab: exemplar picker (doc list with
  checkboxes), "Rebuild profile" button, editable preview of the voice card,
  on/off switch.
- Optional later: few-shot mode — include one short exemplar excerpt
  verbatim in the prompt for stronger imitation at higher token cost.

**Effort:** 2–3 days. **Decisions:** one global profile vs per-doc-type
profiles (start global); auto-suggest exemplars (longest/most-edited docs)
vs manual-only (start manual).

---

## 7. Cross-document memory (search + AI recall)

**What:** Two stages. (a) Full-text search across all docs (table stakes).
(b) The assistant can *consult* the corpus: "what have I written about
pricing?", "link my older post on this" — via a search tool it can call.

**Why:** The corpus becomes a knowledge base. No cloud RAG stack needed —
SQLite FTS5 is built into the bundled rusqlite.

**Implementation:**
- **Stage A — search (medium-low):**
  - Migration: `CREATE VIRTUAL TABLE docs_fts USING fts5(name, content, content_rowid=...)`
    — note: documents use TEXT uuid PKs, so use an external-content-free
    FTS table keyed by doc id and maintain it with triggers
    (INSERT/UPDATE/DELETE on `documents`) or explicit upserts inside
    `save_document_content`/`create_document`/`delete_document` (prefer
    explicit upserts: visible in code, no hidden trigger logic, and the
    storage fns are already the single write path; debounced saves make
    write volume trivial).
  - `search_documents(query) -> Vec<{doc, snippet, rank}>` using
    `snippet()`/`bm25()`. Sidebar search box (`Mod-p`/`Mod-shift-f`),
    results list, click opens doc.
- **Stage B — AI recall (medium-high):**
  - Tool-use loop in `ai.rs`: declare one tool `search_documents` on chat
    requests; when the model emits a tool call, run the FTS query, return
    top-N snippets (capped chars), continue the stream. This makes the SSE
    loop a multi-turn loop — the biggest change to `stream_sse` since v1.
    Implement for Anthropic first (tool use over SSE is well-specified;
    **check the claude-api skill for current streaming tool-use event
    shapes**); OpenRouter tool calling varies by model — gate it.
  - UI: render a small "🔍 searched: 'pricing'" chip in the chat transcript
    when the tool fires, so retrieval is visible, not spooky.
- Explicit context attach (cheap, do with Stage A): "@-mention" a doc in
  the chat input to include its content in the system prompt — no tool loop
  needed and covers half the value.

**Effort:** Stage A 1–2 days; @-mention +1 day; Stage B 3–4 days.
**Decisions:** ship A + @-mention first and defer B until #3's token
visibility shows users the cost of stuffed context.

---

## 8. Pipeline & publishing

**What:** Docs get a workflow status (idea → drafting → review → published)
and a per-platform publish record (where, when, URL). Sidebar gains a
status filter/board view. Direct API publishing for platforms that allow it
(Ghost Admin API, Dev.to, beehiiv; WordPress later), reusing the export
renderers for payload generation. Substack/Medium stay clipboard-only (no
usable API).

**Why:** Closes the loop — idea to published without leaving the window.
With #1's exports, the app now *is* the distribution tool.

**Implementation:**
- Migration: `ALTER TABLE documents ADD COLUMN status TEXT NOT NULL DEFAULT 'drafting'`
  (validated by a Rust enum like `DocType`), plus:
  ```sql
  CREATE TABLE publications (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    platform TEXT NOT NULL, url TEXT, external_id TEXT,
    published_at TEXT NOT NULL
  );
  ```
- Manual record first: "Mark as published on…" in the export flow writes a
  `publications` row (even for clipboard targets — user confirms after
  pasting). This alone delivers the tracking value with zero API work.
- API publishing, one platform at a time (suggest **Ghost** first: clean
  token auth — Admin API key + JWT — markdown/lexical accepted, good docs):
  `publish/ghost.rs` behind a `Publisher`-style interface mirroring how
  `export::TARGETS` registers targets. Render via the one comrak engine
  (HTML renderer output). Credentials per platform go through the existing
  key-storage path in `ai.rs` (keychain release / dev-keys debug) —
  generalize its key names rather than building a second secret store.
- UI: status chip on TopBar (click to advance), sidebar filter pills; a
  board/calendar view is a later polish layer, not v1 of this feature.
- Update `documentTypes.ts`-style conventions: status enum lives next to
  `DocType` in Rust; kebab-case serde.

**Effort:** status + manual records 2 days; first API platform +2–3 days
each. **Decisions:** which platform first (Ghost recommended); whether
"published" status auto-sets on publish (yes).

---

## 9. Content multiplication

**What:** The capstone. From a finished source doc: "Generate the
newsletter version, a LinkedIn post, and an X thread." Each derivative is a
*linked document* (not a throwaway export), platform-native in tone and
structure, written in the user's voice (#6), each individually editable and
exportable/publishable (#1, #8).

**Why:** This is the moat — "write once, publish everywhere" where the app
does the *everywhere* part. Nobody in the markdown-editor space does this.

**Implementation:**
- Schema: `ALTER TABLE documents ADD COLUMN derived_from TEXT REFERENCES documents(id) ON DELETE SET NULL`.
  That's the whole data model: derivatives are ordinary documents with a
  parent pointer; sidebar nests them under the source (one-level, like
  folders today).
- Prompts: one per target doc-type, stored as Rust constants alongside
  `templates.ts` knowledge — "adapt this blog post into a LinkedIn post:
  hook first line, ≤1300 chars before the fold, no headings…" etc. Inject
  the voice card (#6). Use the strong model; these are the big jobs the
  todo.md model-split note anticipated.
- Flow: "Multiply…" button on TopBar → checklist of targets → runs
  generations (sequentially v1 — simpler, and provider rate limits make
  parallel marginal) → creates derivative docs via existing
  `create_document` + content save → progress UI with per-target status →
  sidebar shows the new cluster. Reuse the tagged-stream-id event plumbing;
  each generation is just a stream whose accumulated output lands in a new
  doc instead of a chat.
- Regeneration: re-run one derivative (snapshot it first, #2); show
  "source changed since generated" hint by comparing `updated_at`s.
- A "campaign view": source doc + its derivatives + their publish status
  (#8) on one screen — this is the screenshot that sells the app.

**Effort:** 4–6 days on top of #6 (and it gets better with #7's recall and
#8's statuses, but only #6 is a hard prerequisite). **Decisions:** generate
into editable drafts always (never auto-publish); how much per-platform
knowledge lives in prompts vs renderer post-processing (start prompts-only).

---

## Suggested release grouping

- **v2.0 — "the writing partner":** #1, #2, #3, #4 (exports + safety net +
  chat upgrade + inline edit). Shippable story: "AI edits your text where
  you write, safely."
- **v2.1 — "your voice, your corpus":** #5, #6, #7-A (+@-mention).
- **v2.2 — "the content operation":** #7-B, #8, #9.
