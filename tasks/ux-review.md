# Plume — UX Review (2026-06-15)

Full application UX review: areas that cause a bad user experience. Findings
from a 4-surface parallel code review (editor/shell, AI features, export/backend,
onboarding/settings). Each item cites real code and a one-line fix direction.

**Order of work: Critical → High → Medium → Low.** Critical (data loss / silent
failure) is the current focus.

Legend: `[ ]` todo · `[~]` in progress · `[x]` done

---

## 🔴 CRITICAL — data loss / silent failure (DO FIRST)

> **All 5 fixed 2026-06-15.** `pnpm check` 0/0, `cargo build` green. C1 still
> wants a live `pnpm tauri dev` Cmd+Q check (can't verify quit-flush headlessly).

- [x] **C1. Quit/close loses the last ~500ms of edits.**
  Only flush-on-exit is a `beforeunload` listener (`src/routes/+page.svelte:583`),
  which Tauri v2 doesn't reliably fire on Cmd+Q / native close; `flush()` is async
  so the window tears down before the save resolves. No `onCloseRequested` handler
  exists anywhere (grep: zero hits in `src`/`src-tauri`).
  → Wire Tauri `onCloseRequested`, await the flush, then allow close.

- [x] **C2. Inline-edit "Accept" on an empty/zero-token stream deletes the selection.**
  `accept()` (`src/lib/inlineEdit.svelte.ts:373-398`) allows accept during the
  `streaming` phase and splices `this.streamed` verbatim with no empty guard. A
  provider returning success with an empty body → Accept replaces the selected
  text with `""`. Real data loss. Also no fence-stripping if a weak model emits
  a leading ```` ``` ````.
  → Block accept when `streamed.trim()` is empty; strip stray code fences.

- [x] **C3. Long Multiply/Expand drafts silently destroyed by any other AI action.**
  Single `AiState` slot (`src-tauri/src/ai.rs:566`). Start a multi-target multiply,
  send a chat message → `run_stream` aborts the running task, emits
  `done {aborted:true}`; multiply/expand treat it as a hard failure and discard the
  partial draft (`src/lib/multiply.svelte.ts:52-58`,
  `src/lib/ideaExpand.svelte.ts:50-56`). Nothing tells the user the features are
  mutually exclusive.
  → Disable other AI entry points while multiply/expand runs, or queue instead of abort.

- [x] **C4. "Replace document" from the assistant clobbers the draft with no snapshot, no confirm.**
  `applyAssistantContent` (`src/routes/+page.svelte:166-172`) does a full-document
  replace via `editorView.dispatch`. Unlike Restore it takes no safety snapshot
  first (`AssistantPanel.svelte:249` triggers it). Instant loss of the current draft
  with no surfaced undo.
  → Snapshot before "Replace document"; consider a confirm for full-doc replaces.

- [x] **C5. Doc-switch content load is unguarded against races.**
  `content = await api.getDocumentContent(id)` (`src/routes/+page.svelte:246-257`)
  has no sequence guard, unlike the assistant store's `loadSeq`
  (`src/lib/assistant.svelte.ts:229`). Fast A→B switching can let A's slower fetch
  resolve after B is selected, writing A's text under B's id (editor remounts via
  `{#key selectedDoc.id}`).
  → Add the same monotonic sequence guard the assistant store already uses.

---

## 🟠 HIGH — broken or confusing core flows

> **All 6 fixed 2026-06-15.** `pnpm check` 0/0, `cargo test` 95/95. Live
> `pnpm tauri dev` pass still wanted for: narrow-window stacking (H1), multiply
> Cancel + Inbox-spinner Cancel (H2), and the docx "Exporting…" status (H5).

- [x] **H1. No responsive layout at all.** Zero `@media` queries in the frontend.
  Fixed `grid-template-columns: 260px 1fr` (`src/app.css:189-205`) + hard two-pane
  flex editor/preview. Half-screen / narrow windows are barely usable; no sidebar
  collapse, no stacking.
  → Add breakpoints that collapse the sidebar and stack editor/preview below a width.

- [x] **H2. Multiply & Expand have no cancel.** MultiplyModal close is *disabled*
  while running (`src/lib/components/MultiplyModal.svelte:21,57,114`); Inbox expand
  spinner has no stop (`src/lib/ideaExpand.svelte.ts:75-102`). User is locked into
  up to 4 sequential full-doc generations on the expensive default model. No stream
  timeout either (`ai.rs` client has no `.timeout()`), so a hung connection = forever
  spinner.
  → Add cancel that calls `api.stopAssistant()` + rejects the in-flight promise; add a client idle timeout.

- [x] **H3. Canceling a save dialog gives zero feedback.** `exportTo`
  (`src/routes/+page.svelte:178-195`) has no branch for `result.type === "cancelled"`
  even though Rust deliberately returns `ExportOutput::Cancelled`
  (`src-tauri/src/export/mod.rs:59`, `commands.rs:207`). Silence reads as failure.
  → Add a cancelled branch ("Export canceled" / no-op).

- [x] **H4. First-run: no onboarding; AI setup is undiscoverable.** Empty shelf shows
  four bare verbs (`HomeShelf.svelte:310-324`); only Settings entry is an unlabeled
  gear icon (`HomeShelf.svelte:238-243`). The clear "Set up an AI provider" prompt
  only renders inside the Assistant tab (`AssistantPanel.svelte:163-181`), reachable
  only after opening a doc. Concepts (multiply/expand/inbox/rest) are never explained.
  → Surface a one-time AI-setup nudge + concept hints on the shelf; consider a seed/example doc.

- [x] **H5. Heavy export work runs on the async runtime thread.** docx/html render
  (image decode/re-encode at `src-tauri/src/export/docx.rs:521-539`, zip) runs before
  the first `.await` and before the save dialog spawns (`commands.rs:178-185`). Large
  docs with images stall the app; no "Generating…" indicator anywhere in `exportTo`.
  → `spawn_blocking` the render; show in-progress status before awaiting.

- [x] **H6. Multiply with no API key dead-ends.** MultiplyModal unconfigured state
  (`MultiplyModal.svelte:66-67,119-123`) tells the user to "Add an AI API key in
  Settings" but provides no button to open Settings (unlike AssistantPanel/search).
  → Add an "Open Settings" button to the MultiplyModal unconfigured state.

---

## 🟡 MEDIUM — rough edges & misleading feedback

- [ ] **M1. Raw provider/library error strings shown verbatim.** 401/429/529, disk-full,
  rusqlite/io errors all surface as cryptic opaque toasts (`ai.rs:654-665`,
  `error.rs:7-12`, `docx.rs:100`, `+page.svelte:107,194`). Expired keys mid-session
  give no "re-enter your key" guidance.
  → Map common status codes / error variants to actionable messages.

- [ ] **M2. False "copied" confirmations.** `copyMessage` (`AssistantPanel.svelte:145-149`)
  is fire-and-forget `void navigator.clipboard.writeText` with no `.catch` — shows
  the checkmark even on failure. Clipboard exports (`+page.svelte:179-189`) show
  "Export failed" when the render succeeded and only the OS clipboard handoff failed.
  → Await the assistant copy, flip checkmark only on success; distinguish render-ok from clipboard-fail.

- [ ] **M3. Export errors bypass the toast convention.** Failures flash in a 5-second
  inline status span and vanish (`+page.svelte:193-201`, `showExportStatus`),
  inconsistent with every other failure surface. CLAUDE.md says failures use `toast.error`.
  → Route export failures through `toast.error`; keep inline status for success only.

- [ ] **M4. Menus dismiss only on `mouseleave`.** Shelf "+ New" (`HomeShelf.svelte:206`)
  and TopBar export (`TopBar.svelte:115`) — no outside-click or Escape. Tap/trackpad
  users get stuck menus overlapping content.
  → Close on outside pointerdown + Escape, not mouseleave.

- [ ] **M5. Inline new-project name silently discarded on blur.** `HomeShelf.svelte:329-340`
  sets `creatingProject = false` on blur without committing; type a name then click
  the target to confirm → name lost. Same shape in Sidebar folder rename.
  → Commit on blur if non-empty (as `commitFolderRename` does).

- [ ] **M6. Restore: no confirmation, no "done" feedback.** `HistoryPanel.svelte:70` →
  `+page.svelte:227-237` full-doc swap under the user. It *does* take a safety snapshot
  (good) but says nothing about it. Also "View" (`HistoryPanel.svelte:24-31`) awaits
  `getSnapshotContent` with no spinner.
  → Toast/confirm restore noting the auto-saved pre-restore version; add a View loading state.

- [ ] **M7. No UI to remove a saved API key.** `delete_api_key`/`delete_tavily_key`
  (`ai.rs:240-254`) and `removeKey`/`removeTavilyKey` (`assistant.svelte.ts:416-429`)
  exist but nothing in `src/lib/components` calls them. Can't rotate to no-key without
  hand-editing the keychain / `dev-keys.json`.
  → Add a "Remove key" action in SettingsDialog.

- [ ] **M8. Light-mode theme flash on launch.** Theme read in `onMount` and applied
  after mount (`+page.svelte:561-563`); default `:root` is dark (`app.css:10-77`).
  Light users get a dark flash every launch.
  → Apply stored theme inline in `app.html` before first paint.

- [ ] **M9. Empty document exports as a blank file with a success message.**
  `linkedin::render`→`""`, `x` empty packing, `docx` valid-but-blank; no empty guard
  in `export_document` (`commands.rs:156-188`). "Copied — ready to paste" with nothing
  on the clipboard.
  → Detect empty rendered output, warn "Nothing to export."

- [ ] **M10. Settings: no save confirmation, no validation.** `SettingsDialog.svelte:55-90`
  just closes on success — no "Saved" feedback. Model field (`:136-144`) and keys are
  free-text with zero validation; a typo'd model saves and fails later as a cryptic
  mid-stream API error.
  → Confirm-on-save feedback; optionally validate the key with a test call.

- [ ] **M11. Idea capture friction & silent empty-discard.** `IdeaCaptureModal.svelte`:
  overlay/Escape discards typed text with no guard (`:54`); save is fire-and-forget so
  the modal closes before the save resolves (`:38-54` + `+page.svelte:630`); empty
  idea (`+page.svelte:351`) silently saves nothing with no feedback. Cmd+Enter save
  hint is undocumented.
  → Await save before closing; guard dirty dismiss; toast on empty discard; show the save hint.

- [ ] **M12. Token/context limits surfaced poorly.** OpenRouter `OPENROUTER_HISTORY_BUDGET`
  (120k hard cap) silently drops oldest turns with no UI signal; Anthropic context
  overflow → raw toast. No "approaching limit" warning (`AssistantPanel.svelte:79-88`,
  `assistant.svelte.ts:13-20`).
  → Warn when nearing the budget; tell the user when history was trimmed.

- [ ] **M13. Multiply partial-failure double-reports & can't be stopped mid-batch.**
  One failure doesn't abort the batch (good) but fires a per-target `toast.error` on
  top of the modal's own error row (`+page.svelte:447-465`, `MultiplyModal.svelte:113-116`);
  total-outage = watch 4 spinners fail with 4 toasts, no cancel (ties to H2).
  → De-dupe error reporting; stop the batch early on repeated hard failures.

- [ ] **M14. Inline edit cancelled by a programmatic doc edit with no feedback.**
  History→Restore or chat Insert/Replace during an inline-edit stream resets `ieField`
  to IDLE and aborts with no toast (`inlineEdit.svelte.ts:56-63,413-419`) — preview
  widget just vanishes mid-generation.
  → Toast "Inline edit cancelled — document changed."

- [ ] **M15. Settings dialog footer can scroll below the fold; Save outside the form.**
  `.dialog` is `max-height:85vh; overflow-y:auto` (`app.css:1728-1729`); the long
  settings body pushes the non-sticky footer Save out of view on short windows
  (`SettingsDialog.svelte:116-213`).
  → Make `.dialog-footer` sticky.

---

## 🔵 LOW — polish

- [ ] **L1. No loading state opening a doc / preview flicker.** `getDocumentContent`
  shows the previous doc until the fetch resolves (`+page.svelte:246-257`); preview
  re-`{@html}`-injects wholesale resetting scroll (`Preview.svelte:11`,
  `+page.svelte:132-152`). → Loading state + preserve scroll / patch HTML.

- [ ] **L2. Search failures swallowed.** `catch {}` → `searchResults = []`
  (`Sidebar.svelte:179-187`, `HomeShelf.svelte:77-85`) — a backend error is shown as
  "No matches." → Distinguish error from empty; toast on failure.

- [ ] **L3. Hardcoded colors bypass the design system.** Literal `rgba(74,158,255,…)`
  and `color: white` (`app.css:1808-1809,1825-1827,1927,2254,2370,2415,2426,2536`)
  don't switch for light theme (light accent is `#0969da`). → Use `var(--accent-surface)` / `var(--accent-text)`.

- [ ] **L4. Low-contrast tertiary text.** `--text-tertiary:#666` / `--status-text:#555`
  on dark bg below WCAG AA at 11px (`app.css:29,56-62,169`). → Bump contrast or reserve for larger text.

- [ ] **L5. Icon-only Multiply/Export/Settings/Theme buttons.** Four near-identical
  30px glyphs, app-specific concepts disambiguated only by `title`
  (`TopBar.svelte:94-139`). → Add labels or visually distinguish.

- [ ] **L6. "Rest project" control is `opacity:0` until hover** (`HomeShelf.svelte:155-164`,
  `app.css:502-519`) — the whole resting-projects concept is invisible. → Keep faintly visible.

- [ ] **L7. docx silently drops content.** Remote/relative images degrade to a link
  (`docx.rs:466-485,515-534`); non-paragraph footnote content dropped (`docx.rs:498-509`).
  → Note dropped images in the success message; render non-paragraph footnote blocks.

- [ ] **L8. Token/usage jargon for non-technical users.** "~12,345 tok" / "123 in · 45 out"
  (`AssistantPanel.svelte:196-200,235-236`). → Hide or soften behind a label.

- [ ] **L9. `relativeTime` never ticks** (`HistoryPanel.svelte:33-44`) — "just now" stays
  for minutes. → Recompute on an interval.

- [ ] **L10. Deleting the open doc jumps to Home with no toast** (`+page.svelte:513-526`)
  — feels like the app lost the doc. → Toast "Deleted '<name>'."

- [ ] **L11. Assistant send failure clears the input** (`AssistantPanel.svelte:96-108`,
  `assistant.svelte.ts:377-387`) — rollback restores the thread, not the textarea, so
  the user must retype. → Restore input/mentions on send failure.

- [ ] **L12. Stop before first token leaves an empty user turn** (`assistant.svelte.ts:350-352`)
  — relies on `toApiMessages` merge to stay API-valid; user sees a sent message with no
  reply and no explanation. → Subtle "no response" marker.

---

## Themes / notes

- The **chat path is the most robust** surface (abort handling, `loadSeq` guard,
  key-missing messages, stream-interruption toasts). The rough edges cluster in the
  **headless generative paths (multiply/expand)**, **app-lifecycle save**, and
  **error messaging**.
- Several CRITICAL fixes just apply patterns the codebase already has elsewhere:
  the `loadSeq` guard (C5), the `onCloseRequested` Tauri hook (C1), the `toast.error`
  convention (M3).

## Done well (don't re-investigate)
- `pendingSaves` map preserves failed saves across doc switches with retry.
- Doc + folder delete both confirm; folder delete warns about orphaned docs.
- AI-before-key gives clear messages in chat + web-search (not a crash).
- x-thread packing is robustly bounded under 280 chars (tested).
- Db `Mutex` is NOT held by export or AI streaming — they can't freeze DB-backed UI.
