# Plume — monetization

> Decided 2026-06-10 (Warp-style). Pairs with `tasks/product-direction.md`.
> Price points still TBD; the *model* is set.

## The model — free editor, paid AI + sync

Closed source. **Warp-style:** the app is free; the AI service and sync are the
paid product. **No BYOK.**

### Free tier — the editor
- Free download, **works fully offline, no account required.**
- Local-first markdown notebook: editor, preview, projects/folders, ideas,
  export. **No AI. No sync.**
- Must be a genuinely good notebook *with AI off* — it's the funnel, not
  crippleware. (If it isn't compelling standalone, there's no business.)
- Trust rule: free = open it and write, no signup, no phoning home. An account
  only appears when the user pays. (This is the local-first promise; Warp got
  burned early for breaking exactly this.)

### Paid tier — monthly subscription
Bundles the two things the free tier deliberately lacks:
- **Managed AI** — turnkey, no key, no setup. "AI included."
- **Sync** — corpus across devices.

**No bring-your-own-key.** AI is only available through the paid service. This
keeps the value ladder clean (free = notebook, pay = AI) with no
self-cannibalization, and matches the turnkey thesis (the target user won't have
or understand an API key).

## Billing mechanic — subscription with a hidden allowance

**Flat monthly sub. Tokens are hidden** behind a generous fair-use allowance —
the user never sees a token balance. Reasons:
- Tokens are a developer mental model; exposing them to a non-technical notebook
  user adds the exact cognitive load + usage anxiety we're removing. Metering
  makes people *careful* with the AI — the opposite of what drives love/retention.
- The cost basis (cheap model, below) makes per-token precision pointless: usage
  cost per normal user is tiny, margin is fat, and breakage (under-use) helps.
  We can afford "use it freely."
- Subscription = predictable MRR, stickiness, and clean bundling with sync.

### Heavy-user tail (the one real risk in a flat sub)
Protect margin without exposing tokens to the 95%:
- A **generous internal cap** (fair use) that almost no one hits.
- A **safety valve** only past the cap: auto-bump to a higher tier *or* a metered
  overage top-up. Metering exists only at the extreme tail, never for normal use.

### Trial
Optional small **one-time AI grant** for new users — feel the AI before the
sub wall. A conversion tool, not a model. (Prepaid credits are otherwise NOT
used — wrong wrapper for a consumer audience.)

## AI economics

- **Resell a low-cost model at a markup** (white-label the provider usage, add
  margin). Test model: **MiMo-V2.5-Pro**.
- **Self-host later** once user volume justifies owning the inference (better
  margin + control). Treat this as a serious infra undertaking, not a quick
  step — resell as long as the margin math allows.
- The markup mechanic is the same under any wrapper; the decision was the
  *packaging* (hidden-allowance subscription for a consumer audience).

### Cost basis — MiMo-V2.5-Pro (as provided 2026-06-10)
> Model pricing drifts — reverify before setting prices.
- Model: XiaomiMiMo / MiMo-V2.5-Pro — https://huggingface.co/XiaomiMiMo/MiMo-V2.5-Pro
- Context: 1.05M
- Input: **$0.435 / M tokens**
- Output: **$0.87 / M tokens**
- Cache read: **$0.0036 / M tokens**

## Open source — later lever, not now

Open-sourcing the *client* is compatible with this model (the revenue lives in
the hosted AI + sync service, which the client can't self-replicate — same logic
that let Warp open-source its terminal). But **not now**: pre-traction and solo,
it buys little and costs flexibility. Hold it as a future trust/adoption lever,
pulled when opening up would measurably accelerate growth.

## Open / TBD
- Subscription **price point** (model against the MiMo cost basis + breakage).
- Fair-use **allowance size** + where the safety-valve cap sits.
- **Sync** infra + whether it's ever separable from the AI sub (default: bundled).
- **Trial** grant size.
- **Self-host trigger** — user count / margin threshold.

## Inbox — add monetization notes here
<!-- price points, competitor pricing, allowance math, etc. -->
-
