# Plume — product direction (2026-06-10)

> A strategy shift captured mid-build. This **supersedes the distribution-first
> framing** in `tasks/v2-roadmap.md` (which is still accurate as a record of
> what shipped, but no longer the north star). Read this first next session.

## The shift, in one line

From **"write once, publish everywhere"** (a distribution tool) → to
**a markdown workspace for building in public**: plan a project, keep its build
log, and turn that real work into quality posts — in your own voice. Output is
copy/paste + export. **No publishing pipeline.**

## Why

- The distribution pipeline (direct API publishing to Ghost/beehiiv/Dev.to —
  old #8) is an infinite **solo-maintenance treadmill** (every platform's API
  breaks forever) for the **least differentiated** part. Cut it.
- The real pull is the **writing** — quality content grounded in real work, the
  opposite of mass-produced AI slop.
- **Dogfooding is the strongest signal:** the builder *is* the target user.
  Markdown is where everything in AI/dev now lives (skills, agents, CLAUDE.md,
  plans, specs, READMEs). The audience that lives in those files has no tool
  connecting "I built this" → "here's the post about it." That gap is the wedge.

## The audience & job-to-be-done

Technical creators / indie hackers / devrel / founders doing **build-in-public**.
The job: *"help me document what I'm building and turn it into good writing,
fast, in my voice."*

## How existing features re-frame (nothing wasted)

- **Content multiplication (#9)** is NOT "mass-produce for channels I don't
  have." It's **"turn my build log / plan / notes into a post."** Same engine;
  the source is now *your real work in your voice* → the anti-slop story.
- **Cross-doc search + @-mention (#7A)** = **build memory** — pull from past
  specs/logs into new writing.
- **Voice & tone (#6)** = your authentic technical voice on the page.
- **Idea inbox (#5)** = capture, then expand into a plan or a draft.

## The home screen — "the notebook shelf" (Projects-led)

When you're in the tool but not in a document, you land on the **home/shelf**,
not a blank editor and not a dashboard. **Decided direction: Projects-led.**

```
┌─ Plume ──────────────────────────────────┐
│  Your notebook                 + New ▾    │
│  ▸ Plume                        6 docs    │
│    plan · build-log · blog:#9 · …         │
│  ▸ Side experiments             2 docs    │
│  ▸ Writing                      4 docs    │
│  Inbox ──────────   Recent ─────────────  │
│  · markdown trends  · polish plan    2h   │
│  · flywheel idea    · FTS plan       3h   │
└───────────────────────────────────────────┘
```

Design law: **calm, not busy.** Feels like flipping open a notebook, not a SaaS
dashboard. Gets out of the way the moment you pick a thing.

### Three decisions the shelf implies
1. **A project = a folder.** Do NOT invent a `projects` table. Folders already
   exist; the shelf is a richer *view* over them. (One concept.)
2. **Active vs. resting** is the entire "planner." Active projects sit up top;
   resting ones collapse/archive away so the shelf stays calm. No task lists, no
   due dates, no statuses — "active" is the whole curation.
3. **Loose pages + ideas need a seat.** Ideas → the Inbox (built). Unfiled
   single docs → a "Loose pages" row or just Recent. Decide, don't leave them
   homeless.

### Plan-mode front door
`+ New ▾` = `New project · New doc · New idea · New plan`. **"New plan"** is the
entry to the build-in-public loop (plan → build log → write). Keep plan-mode
**lightweight markdown docs pointed at the writing** — NOT a PM tool.

## The discipline (the trap to avoid twice)
A project stays **a folder of markdown.** The moment statuses / assignees /
kanban / due dates tempt you, that's the pipeline mistake wearing a planning
costume. The shelf shows *what exists and what's fresh*, never a workflow engine.
Same rule that says "cut the publishing pipeline" says "don't rebuild Linear."

## Monetization (open, but a lean)
Local-first + bring-your-own-key = ~no server cost and ~no recurring value to
hang a subscription on. So:
- **Lean: one-time purchase** (indie Mac-app model — Things / iA Writer). Fits
  local-first; paid major-version upgrades for recurring revenue; no churn or
  infra to manage solo.
- **Future optional recurring:** encrypted **sync/backup** (corpus on laptop +
  desktop). The one thing this audience pays recurring for — and it also fixes
  the "desktop-only" friction. Does NOT drag you back into publishing APIs.
- **Marketing writes itself:** build Plume in public, using Plume.

## Risks to keep honest
- Adjacent threat isn't a competitor, it's **"just use Obsidian + ChatGPT."**
  Defensibility = the tight integrated loop (plan → log → post → voice → corpus)
  no single tool does. The loop has to beat the patchwork on *flow*, not feature
  count.
- **Make-or-break = adaptation quality.** Are the multiplied drafts post-ready,
  or do they need heavy rewriting? If post-ready → daily habit. If not → clever
  utility. This is why the **per-platform prompt quality** in the polish pass is
  the highest-leverage work left.
- Desktop-only is an adoption tax until/unless optional sync exists.

## Open questions for next session
- What exactly is a "build log" doc vs. a plain doc — a convention, a template,
  or a first-class doc type?
- Does "active" live on the folder (an `active` flag) or is it a separate pinned
  set? (Lightest: a boolean on folders.)
- Loose pages: dedicated shelf row, or fold into Recent only?
- Sequence: home/shelf first, or polish the writing/adaptation quality first?
  (Quality is higher-leverage; the shelf is the more *visible* win.)

## Related docs
- `tasks/polish-backlog.md` — the polish items (compaction, assistant hardening,
  docx, + the user's inbox of small things).
- `tasks/v2-roadmap.md` — historical record of #1–#9; distribution framing now
  superseded by this note. #8 publishing → effectively **cut** (was already
  parked in "v2.5").
