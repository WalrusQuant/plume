# Plume — monetization

> Decided 2026-06-10 (Warp-style sub). **Reversed 2026-06-11:** BYOK + flat
> one-time fee; sync deferred. Pairs with `tasks/product-direction.md`.
> Price point still TBD; the *model* is set.

## The model — buy it once, bring your own key

Closed source. **One-time purchase unlocks the full app.** AI runs on the
user's **own API key (BYOK)**. No subscription, no managed AI service, no token
reselling, no sync (for now).

Rationale for the reversal from the Warp-style sub:
- **Solo + pre-traction, the sub model is mostly infra I don't have.** Managed
  AI = a billing system, a metering/allowance system, a provider-resale margin
  to defend, a heavy-user safety valve, and fraud/abuse surface. One-time +
  BYOK deletes all of it. I ship the app, not a service.
- **BYOK is honest with the audience.** It's 2026 — a writer who wants AI can
  get an API key. "Be an adult and get a key" is a fine ask for a paid tool,
  and it sidesteps the whole "am I being metered?" anxiety the sub model was
  trying to hide.
- **Zero variable cost per user.** No inference bill, so no margin math, no
  breakage modeling, no model-price drift risk. Revenue is just price × units.
- **Matches what's already built.** AI keys already live in the OS keychain
  (release) / `dev-keys.json` (debug) and never touch the webview. BYOK is the
  current architecture — the sub model would have been net-new backend.

### What you get for the one-time fee
- The full local-first markdown notebook: editor, preview, projects/folders,
  ideas, cross-doc search, export.
- The AI writing partner (chat, inline edit, idea expand, multiplication) —
  **powered by your own Anthropic / OpenRouter key.**
- All future updates within the major version (define the boundary before
  launch — see Open/TBD).

### Trust rule (unchanged, and easier now)
Open it and write — no signup, no account, no phoning home. With BYOK there's
not even a managed-AI login: the only "account-like" thing is the user's own
provider key, which they control. This *is* the local-first promise, and the
one-time model keeps it cleanly.

## Pricing mechanic — flat one-time fee

- **Single flat price, one-time.** No tiers at launch (a Pro/Personal split is
  a later lever, not a launch requirement).
- License/activation: a license key or simple offline-friendly activation.
  Keep it lightweight — the app must still work offline and not feel like DRM.
  (Decide the exact mechanism before launch — see Open/TBD.)
- **No metering, no allowance, no overage.** The entire hidden-token apparatus
  from the sub model is gone. The user's provider bills the user directly.

## Sync — deferred, demand-gated

**Not built now.** Sync was the other half of the old paid bundle; without the
sub it has no billing to ride on, and it's real infra (conflict handling,
hosted storage, auth). Defer it.

- Build sync **only if buyers actually ask for it** after purchase.
- If/when it ships, decide then whether it's a paid add-on, a paid major-version
  bump, or bundled. Don't pre-commit.
- Until then, "local-first, single-device" is the honest framing. Desktop-only
  is an adoption tax we're accepting at launch (see product-direction risks).

## AI economics — none to manage

There is no resale margin, no cost basis, no model to white-label. The user
pays their provider; Plume takes no cut of inference. (The old MiMo-V2.5-Pro
resale cost basis is retired — kept below only as a historical note in case the
managed-AI question ever reopens.)

<details>
<summary>Retired: managed-AI cost basis (MiMo-V2.5-Pro, 2026-06-10)</summary>

Only relevant if Plume ever revisits reselling inference. Pricing drifts —
reverify before trusting.
- XiaomiMiMo / MiMo-V2.5-Pro — https://huggingface.co/XiaomiMiMo/MiMo-V2.5-Pro
- Context 1.05M · Input $0.435/M · Output $0.87/M · Cache read $0.0036/M
</details>

## Open source — later lever, not now

Unchanged from before. Open-sourcing the client is *more* compatible with this
model than the last one (there's no hosted service to protect — the "moat" is
just the product and brand). But still **not now**: pre-traction and solo it
buys little and a paid one-time binary wants to stay closed at launch. Hold it
as a future trust/adoption lever.

## Open / TBD
- **Price point** for the one-time fee (anchor against one-time creative/writing
  tools, not SaaS subs).
- **License/activation mechanism** — key file, online check, or honor-system;
  must preserve offline use and not feel like DRM.
- **Update boundary** — what a one-time purchase entitles (lifetime of major
  version vs. lifetime of app vs. N years). Define before launch.
- **Sync trigger** — what level of buyer demand justifies building it, and how
  it's then priced.
- Whether a later **Pro tier** (one-time, higher price) is worth the
  segmentation, or it stays a single SKU.

## Inbox — add monetization notes here
<!-- price points, competitor pricing for one-time creative tools, etc. -->
-
