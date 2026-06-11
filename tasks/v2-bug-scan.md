# v2 bug scan — 2026-06-11

Full manual review of every v2 roadmap change (`b3f8322..HEAD`, ~4,500 lines:
storage.rs, ai.rs, commands.rs, export/x.rs, lib.rs, the four stream
controllers, +page.svelte, Sidebar, AssistantPanel, MultiplyModal,
IdeaCaptureModal, HistoryPanel, api.ts, buildSidebarTree). Baseline at scan
time: 59/59 cargo tests green, `pnpm check` 0 errors / 0 warnings.

Recommended fix order: #1 first (one backend change kills the whole
silent-truncation class), then #2, then the pre-existing UTF-8 decoder since
multiply/expand now persist stream output into documents.

---

## High — real bugs with user-visible damage

### 1. Aborted AI streams resolve as success → truncated content saved
- [x] Fixed — done payload carries `aborted: true` on abort (run_stream takeover + stop_stream); expand/multiply reject, inline edit discards, chat toasts.

`run_stream` (`src-tauri/src/ai.rs:424-426`) aborts any in-flight stream and
emits a plain `assistant:done` for the old stream id. Every v2 listener treats
`done` as success:

- **Idea expand** runs in the background with the UI fully usable (only a
  sidebar spinner). If the user sends a chat message, runs an inline edit, or
  starts a multiply while expanding, the expand stream is aborted,
  `ideaExpand` resolves with the partial text
  (`src/lib/ideaExpand.svelte.ts:48-51`), and `expandIdea` **creates a
  truncated draft document with a "Draft created" success toast**
  (`src/routes/+page.svelte:361-364`).
- **Chat**: the TopBar "Multiply…" button is clickable while a chat reply is
  streaming; starting a run silently truncates the reply, which is then
  persisted to the DB as a complete assistant message
  (`src/lib/assistant.svelte.ts:94-98`).
- **Inline edit**: a chat send mid-edit aborts the edit stream; the controller
  flips to "review" presenting the partial replacement as complete
  (`src/lib/inlineEdit.svelte.ts:267-271`) — Accept splices truncated text
  into the document.

The single-slot mutual-exclusion design is fine; the bug is that abort reuses
the success event. **Fix:** add an `aborted` flag to the done payload (or a
separate `assistant:aborted` event); the resolve-with-text controllers
(expand, multiply) should reject on it, chat should mark the message
truncated, inline edit should discard instead of entering review.

### 2. Inline edit can leave the editor locked read-only with no visible way out
- [x] Fixed — readOnly is now a facet derived from ieField (every field reset unlocks); a doc change under an active edit aborts the orphaned stream via an updateListener.

`EditorState.readOnly` blocks typing but **not** programmatic dispatches.
While an inline edit is streaming, the user can still use History → Restore
or the chat panel's Insert / Replace document buttons (both dispatch into the
editor). That doc change resets `ieField` to IDLE
(`src/lib/inlineEdit.svelte.ts:56`) but the `readOnlyComp` compartment is
never unlocked — unlock only happens in `accept`/`reject` (lines 361, 374).
Worse, while the orphaned stream is still running, phase is "idle", so the
Escape handler returns false (line 217) and there is *no* unlock path at all;
after the stream finishes, phase becomes "review" with `from == to == 0`, so
the Accept/Reject widget never renders — only an unprompted Esc press
recovers. The selection menu also stops appearing.

**Fix direction:** disable restore/apply while an inline edit is active,
and/or unlock the compartment in the same field-reset path that handles
`tr.docChanged`.

### 3. Inline edit Accept can double-apply
- [x] Fixed — `accepting` re-entrancy guard around the snapshot await.

`accept()` awaits `createSnapshot` between the phase guard and the dispatch
(`src/lib/inlineEdit.svelte.ts:344-359`), and the Accept button is never
disabled. A double-click passes the guard twice (the field is still "review"
until the first dispatch lands) and dispatches the replacement twice — the
second at stale offsets, duplicating text. Needs a re-entrancy guard.

---

## Medium

### 4. Stopping a chat before the first token → consecutive `user` roles
- [x] Fixed — API payload merges consecutive same-role messages; errors surface as toasts instead of persisted pseudo-messages.

`stop()` (`src/lib/assistant.svelte.ts:222-228`) persists the thread as-is.
If no assistant token arrived, the thread ends with a `user` message; the
next send produces `[…, user, user]`, which the Anthropic Messages API
rejects (roles must alternate). The user gets an opaque API error on a thread
that looks fine. Related: `Error: …` pseudo-messages (lines 105, 216) are
persisted and replayed to the model as genuine assistant turns, polluting
context.

### 5. `assistant.loadFor` has no sequence guard
- [x] Fixed — `loadSeq` monotonic guard checked after each await; state applied atomically at the end.

Rapid doc switching interleaves two `loadFor` calls
(`src/lib/assistant.svelte.ts:112-129`): each awaits
`listChats`/`getChatMessages` after setting `this.docId`, so the slower fetch
can win and display doc A's chats/messages while doc B is open. The search
box makes rapid switching easy. Add an "is this still the requested docId"
check after each await (the Sidebar's search effect already does this
correctly with `searchSeq`).

### 6. Multiply mutates the workspace before generating anything
- [x] Fixed — folder creation + source move deferred to the first successful draft (`ensureFolder`).

`multiplyDocument` creates the folder and moves the source first
(`src/routes/+page.svelte:402-407`). If every generation then fails (no
network, bad key), the user's doc has still been relocated into a new folder
named after itself — a surprising side effect of a failed operation. Consider
deferring folder creation until the first draft succeeds.

### 7. Persisting a chat rewrites all rows and bumps `updated_at` unconditionally
- [x] Fixed — identical thread is a no-op (no reorder); unchanged rows keep their original `created_at`; regression test added.

`save_chat_messages` (`src-tauri/src/storage.rs:617-637`) runs on every
doc/chat switch even with zero changes: all `created_at` values are reset to
"now" and the chat jumps to the top of the sort order just from being opened.
Destroys real timestamps and makes "most recent chat" ordering meaningless.

### 8. Verify: `"thinking": {"type": "adaptive"}` hardcoded for every Anthropic model
- [x] Verified (claude-api skill, 2026-06-11): Haiku 4.5 does NOT accept adaptive thinking — confirmed bug. Fixed: `supports_adaptive_thinking()` gates the field (Opus 4.6+/Sonnet 4.6/Fable-Mythos 5); omitted for everything else.

`src-tauri/src/ai.rs:574` sends adaptive thinking for all models, including
the `claude-haiku-4-5` fast-model fallback (`ai.rs:55`). If Haiku 4.5 doesn't
accept adaptive thinking, any flow that hits the fast-model fallback (model
field cleared in Settings → inline edit) 400s. Per CLAUDE.md, check against
the claude-api skill — flagged as a verification task, not a confirmed bug.

---

## Minor / edge

- [x] **x.rs numbering overflow** (fixed — packing limit threaded through, re-pack when the suffix would exceed 280; test added): posts are packed to ≤270 chars, then
  `number()` (`src-tauri/src/export/x.rs:418-428`) appends `\n\nn/N`. At
  ≥1000 posts the suffix is 11+ chars and posts exceed 280. Theoretical
  (a ~270k-char doc); the test only asserts ≤280 for small threads. Code
  blocks already overflow by documented design.
- [x] **AssistantPanel double-Enter race** (fixed — input consumed synchronously before the await): `handleSubmit` awaits
  `buildReferences()` after the `isStreaming` check
  (`src/lib/components/AssistantPanel.svelte:96-105`); a second Enter in that
  window clears the input but the message is silently dropped by `send()`'s
  guard.
- [x] **`deriveTitle`/`deriveIdeaName` `slice(0, 40)`** (fixed — code-point slice) can split a surrogate
  pair (emoji at the boundary → broken character in the title).
- [x] **`fts_query` strips intra-word punctuation** (accepted tradeoff — no change): "don't" → `dont`,
  "c++" → `c` — safe (that's its job) but some legitimate queries can't
  match. Acceptable tradeoff; noting it.
- [x] **IdeaCaptureModal discards typed text on overlay click/Esc** (intentional — no change) with no
  confirm — intentional ("no empty ideas"), but silent data loss after a
  long note.
- [x] **OpenRouter headers** (fixed — `x-title: Plume`, referer github.com/WalrusQuant/plume) still say `x-title: "Markdown"` /
  `github.com/adamwickwire/markdown` (`src-tauri/src/ai.rs:635-636`)
  post-Plume rename — cosmetic.
- [x] **`lastSnapshotAt` Map** (fixed — evicted in deleteDocument) in +page.svelte never evicts deleted doc ids —
  trivial leak.

---

## Pre-existing (v1 code, now load-bearing for v2)

### SSE chunks decoded with `String::from_utf8_lossy` per network chunk
- [x] Fixed — bytes buffered in `Vec<u8>`, decoded per complete line (`drain_sse_lines`); split-multibyte test added.

`src-tauri/src/ai.rs:464` (dates to the v1 core commit). A multi-byte UTF-8
character split across two network chunks becomes `�` replacement characters
in the streamed text. In v1 this only garbled a chat bubble; in v2 the same
stream output is **written permanently into documents** by expand and
multiply. Fix: buffer bytes (`Vec<u8>`) and decode incrementally, only up to
the last complete character.
