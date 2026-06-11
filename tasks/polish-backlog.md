# v2 Polish Backlog — pre-release

> Created 2026-06-10. The v2 **feature set** is complete (#1–6, #9, #7A all
> shipped). What's left before cutting the v2 release is polish — all local, no
> new external dependencies. This doc is the staging area for a larger polish
> plan: seeded with the known items below; **add notes under "Inbox" as they
> come** and we'll shape them into a sequenced plan next session.

## Known items (from the roadmap)

### 1. Context compaction — chat threads
Long chat threads will blow up token cost and eventually overflow the context
window. Need to summarize or truncate older turns.
- Anthropic has server-side compaction (beta `compact-2026-01-12`) — check the
  claude-api skill for current support before relying on it.
- OpenRouter path needs a client-side strategy (summarize-and-replace older
  turns, or drop beyond a cap).
- The new per-message token counts (#3) already give us the signal to decide
  when to compact.

### 2. Assistant hardening pass
Chat is a core surface the user wants to invest "a lot" in — treat it as
first-class, not a bolt-on. (Specifics TBD — collect concrete pain points in
the Inbox below.)

### 3. Docx export polish — round 2
Fonts / spacing / tables shipped 2026-06-09; a second pass was deferred pending
real-world feedback. Collect specific docx issues in the Inbox.

---

## Inbox — add small polish items here
<!-- Drop quick notes as you think of them; we'll triage + sequence these into
     the plan next session. Format loosely, e.g.:
     - [area] what's wrong / what you want — any detail
-->

-

---

## Plan (to be built next session)
<!-- After triaging the Inbox we'll turn this into a sequenced, checkable plan. -->
_TBD — build the sequenced polish plan from the items above._
