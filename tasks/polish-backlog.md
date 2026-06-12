# v2 Polish Backlog — pre-release

> Created 2026-06-10. Triaged + sequenced 2026-06-11. The v2 **feature set** is
> complete (#1–6, #9, #7A all shipped); the home shelf + sidebar drag-reorder
> also shipped (migration v8). What's left before cutting the v2 release is
> polish — all local, no new external dependencies. The sequenced plan is at the
> bottom; the three known items are detailed here with grounded findings from
> the 2026-06-11 investigation.

## Known items (from the roadmap)

### 1. Context compaction — chat threads
Long chat threads blow up token cost and eventually overflow the window.
**Current state:** there is *no* truncation or compaction — `send()`
(`assistant.svelte.ts:229`) snapshots the full thread and `run_stream`
(`ai.rs:410`) passes it through verbatim. Worse, the **entire document body +
every `@`-referenced doc** is re-embedded in the system prompt on every turn
(`ai.rs:207-215`), which is usually the dominant cost and is invisible to the
per-message token accounting.

Resolved approach:
- **Anthropic:** server-side compaction, beta header `compact-2026-01-12`
  (supported on Opus 4.8/4.7/4.6, Sonnet 4.6, Fable 5). Default trigger ≈150K
  tokens. **Critical:** append the full `response.content` back each turn — the
  compaction blocks carry the state; appending only the text silently loses it.
- **OpenRouter:** no server-side path — client-side strategy (drop or
  summarize-and-replace older turns beyond a token cap).
- The per-message token counts (parsed from SSE in `ai.rs:524-573`, stored in
  `chat_messages.input_tokens/output_tokens`) already give the signal for *when*
  to compact.
- **Cheap adjacent win — prompt caching.** The static instruction block sits
  *before* the volatile doc content in `system_prompt` (`ai.rs:207`), so nothing
  caches today. Reorder (stable prefix first, doc after a `cache_control`
  breakpoint) or move the doc into a `messages` block. Cache reads are ~0.1×
  input price; on a re-sent doc that is a large recurring saving.

### 2. Assistant hardening pass
Chat is a core surface the user wants to invest "a lot" in. The investigation
turned up concrete **correctness bugs**, not just polish:
- **Aborted/errored streams persist as real assistant turns.** On
  `assistant:done {aborted:true}` the store still `persist()`s the truncated
  text (`assistant.svelte.ts:116-124`); that fragment is later replayed to the
  model as a genuine assistant message. Same for a stream that errors after
  emitting some tokens.
- **Optimistic user message persists on send failure.** The user turn is
  appended before the backend call; if `sendAssistantMessage` throws (`:249`)
  the orphan user message stays and is saved on the next write, then merges into
  the retry via `toApiMessages`.
- **One stream slot shared across surfaces.** Starting an inline edit / idea
  expand / multiply silently aborts an in-flight chat reply (`run_stream:424`),
  surfaced only as a post-hoc toast.
- **`persist()` is a full-thread delete+reinsert** every turn/stop/switch
  (`storage.rs:712`) — O(thread) writes, and not transactionally tied to the
  `loadSeq` guard that protects against doc-switch races.
- **Auto-title fires fire-and-forget** before send (`:233`); if send fails the
  chat is already renamed from an exchange that never happened.

### 3. Docx export polish — round 2
Fonts/spacing/tables shipped 2026-06-09. Round-2 gaps found in
`export/docx.rs` (all elements parse from the shared comrak AST; fixes go in the
renderer, never by toggling comrak options — `preview::options()` is shared with
the live preview):
- **Images silently dropped** — no `NodeValue::Image` arm; alt text leaks as
  plain text (`docx.rs:209`).
- **Links aren't real hyperlinks** — rendered as `text (url)` literal
  (`docx.rs:205`); autolinks produce ugly `text (text)`.
- **Footnotes & task-list checkboxes unhandled** — both extensions are enabled
  in comrak but have no arm; checkbox state is lost, footnotes render as stray
  text.
- **Tables ignore column alignment** (`TableAlignment` discarded, `docx.rs:101`)
  and set no explicit borders/width.
- **Lists use literal `"•  "` / `"N.  "` text markers** + faked indent instead
  of Word-native numbering (`docx.rs:142-185`).
- **Headings are manual bold+size**, not Word built-in Heading styles → no
  outline/TOC.
- **Code/inline-code have no shading**, thematic break is a `"———"` text run,
  fonts/sizes are all hardcoded constants (`docx.rs:11-22`).

---

## Plan — sequenced

Rationale for the order: **fix the correctness bugs first** (a thread full of
truncated/orphaned turns poisons both the UI and the model context, so there's
no point optimizing the token cost of corrupt threads), **then cut cost**
(caching is a quick win, compaction is the larger build), **then the
independent feature polish** (docx). Each phase is shippable on its own.

### Phase A — Assistant hardening (correctness; do first) ✅ DONE 2026-06-11
- [x] Do **not** persist aborted/errored streams as assistant turns — a
      `streamErrored` flag (set by the error handler) + the `aborted` flag now
      drop the trailing partial via `dropTrailingAssistant()` before `persist()`
      (`assistant.svelte.ts` done/error handlers).
- [x] Roll back the optimistic user message when `sendAssistantMessage` throws
      (`send()` catch block strips the trailing user turn).
- [x] Gate auto-title on a successful send — moved after the awaited
      `sendAssistantMessage`, so a failed send no longer renames the chat.
- [x] Stream-slot policy: **kept single-slot, surface pre-emption clearly.**
      Splitting slots is a real architectural change (one HTTP stream + one
      `AiState` handle, shared `assistant:*` events); the correctness fix is the
      toast + dropped partial, which now makes pre-emption non-destructive.
- [~] (Deferred, perf only) Incremental `persist()` — `save_chat_messages`
      already runs in a transaction with an identical-thread short-circuit; the
      O(thread) rewrite is a non-bug nit on small local threads. Not worth the
      regression risk for a single-user app. Left as-is.
- [x] Verified via `pnpm check` (0/0) — assistant store is TS, no JS test
      runner in the project; the Rust caching change has a new cargo test.

### Phase B — Token cost: prompt caching + history cap ✅ DONE 2026-06-11
- [x] **Prompt caching (Anthropic chat).** `anthropic_request_body` now wraps
      the system prompt in a `cache_control: ephemeral` content block, gated by
      a `cache_system` flag threaded through `run_stream`. Chat passes `true`
      (system prefix = instructions + doc re-sent every turn → reads back at
      ~0.1× when the doc is unchanged); inline/expand/multiply pass `false` (no
      write premium on single-use prompts). `anthropic_usage` already folds
      `cache_read`/`cache_creation` into the input count, so the token counter
      stays correct. New test `anthropic_caches_system_only_when_requested`.
- [x] **History cap (both providers).** `capHistory()` trims the *sent* payload
      to `HISTORY_TOKEN_BUDGET` (≈120K est. tokens) — keeps the newest turns,
      never starts on an assistant turn, leaves the stored thread + UI intact.
      Applied in `send()` before `toApiMessages`. Provider-agnostic, no schema
      change.
- [x] **Server-side Anthropic compaction (`compact-2026-01-12`) — DONE
      2026-06-11** (the "master chat per document" case). Migration **v9** adds
      `chat_messages.raw_content`; an assistant turn that carries a compaction
      summary stores + replays its full content-block array verbatim (the API
      drops everything before the compaction block, so only that turn needs
      faithful round-tripping). Backend: `supports_compaction` gate (Anthropic +
      supported model), `context_management` + `anthropic-beta` header behind a
      `compact` flag, SSE `compaction_delta` captured in a `Mutex` and emitted as
      a new `assistant:content` event on the Ok path only (aborted/errored turns
      drop the partial — Phase A). Frontend: `rawContent` round-trips through
      `toApiMessages` (propagated in both branches; block turns never merged),
      persists via `save_chat_messages`, replays as blocks with `cache_control`
      on the compaction block. `capHistory` is now provider-aware: ~120K hard cap
      for OpenRouter, ~600K backstop for Anthropic (compaction does the real
      work at 150K). Plan: `~/.claude/plans/bright-stargazing-patterson.md`.
      **Remaining: one live `pnpm tauri dev` check** — drive a chat past ~150K
      input and confirm the compaction block appears, continuity holds across a
      reopen, and per-turn cost stops growing.
- [x] Context-size affordance: the existing `~X tok` counter in
      `AssistantPanel` (input+output of the last turn) already surfaces growth;
      left in place.

### Phase C — Docx export round 2 (independent feature polish) ✅ DONE 2026-06-11
Full rewrite of `export/docx.rs` (all in the renderer; `preview::options()`
untouched). Added direct deps `image` + `base64` (already in the lock via
docx-rs) and a `zip` dev-dep for unzip-based assertions. 12 docx tests inflate
the produced .docx and assert the OOXML markers; 90 cargo tests + clippy
`-D warnings` green.
- [x] Real images — `NodeValue::Image` arm embeds data-URI + absolute/`file://`
      local images via `Pic::new` (validated through `image::load_from_memory`,
      which also guards Pic's internal panic), scaled to ≤600px. **Remote
      `http(s)` images fall back to a real hyperlink** (alt text) — a sync export
      must not block on the network; remote fetch deferred (see below).
- [x] Real hyperlinks — `Hyperlink::new(url, External)` + blue/underline runs via
      `Paragraph::add_hyperlink`; the ` (url)` literal is gone. URL lands in
      `document.xml.rels`.
- [x] Task-list checkboxes (`☐`/`☑`, detected via `NodeValue::TaskItem`, not
      numbered) and **real Word footnotes** (`Run::add_footnote_reference`;
      definitions collected up front and skipped as body text).
- [x] Table column alignment (`NodeTable.alignments` → `Paragraph::align`),
      default single borders, fixed content width, and header-row shading
      (`D9D9D9`).
- [x] Word-native list numbering — one `AbstractNumbering`/`Numbering` per list
      node (indent encodes depth; ordered lists restart at their `start`); literal
      `"•  "`/`"N.  "` markers + faked indent dropped.
- [x] Built-in Heading1–6 styles (`Docx::add_style`, `name "heading N"` +
      `outlineLvl`) → outline/TOC; headings use `.style("HeadingN")`.
- [x] Code block → shaded borderless single-cell table; inline code → mono font +
      `F2F2F2` run shading; thematic break → bottom-bordered single-cell table
      (real rule); `RunFonts` now sets `ascii`/`hi_ansi`/`cs` for body + mono.
- [~] (Optional, deferred) Configurable fonts/sizes/spacing — left as constants;
      no requirement for a single-user local app.
- [x] 12 unzip-based export unit tests (headings, numbering, start value, task
      checkboxes, hyperlinks, footnotes, table alignment+shading, code shading,
      data-URI embed, remote-image fallback, inline-code mono).

**Deferred follow-up:** fetch remote `http(s)` images and embed them (needs async
+ timeout/error handling so a dead URL can't hang export); currently they become
hyperlinks. Mixed task/non-task lists render every item as a checkbox.

---

## Inbox — add small polish items here
<!-- Drop quick notes as you think of them; triage into the plan above. -->

-
