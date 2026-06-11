# Lessons

## 2026-06-10 — Honor the user's selected model in every AI flow; never hardcode `model: null`
- **Mistake:** idea-expand, content-multiply, and inline-edit controllers passed `model: null` to the backend, which falls back to `provider.default_model()` (OpenRouter `anthropic/claude-opus-4.8`). The user had a cheap model (MiMo) selected but every Multiply/Expand silently ran **Opus** — real money, caught only by checking OpenRouter usage.
- **Rule:** A model the user explicitly selects in Settings must be used by **all** AI surfaces (chat, expand, multiply, inline). Pass `assistant.settings.model || null`, not `null`. A "use a stronger/cheaper model for job type X" optimization must never silently override the user's explicit choice — if that tiering is wanted, make it a visible setting. When reviewing an AI call site, check what model actually reaches the provider.

## 2026-06-10 — Don't gate features behind keyboard shortcuts; make them visible
- **Mistake:** Shipped inline AI edit behind a hidden `Mod-J` shortcut (plus `Mod-Enter`/`Esc`). User selected text, "nothing happened," asked "where is this Mod-J," then: "I hate all these short keys."
- **Rule:** This user dislikes keyboard shortcuts and undiscoverable affordances. Default to **visible, click-driven UI**: trigger on an obvious user action (e.g. show a selection menu when text is selected) and make every action a button. A shortcut is at most an unadvertised convenience (Esc to cancel), never the only way in. When a plan proposes a keybinding as the entry point, push back or add a visible affordance.

## 2026-06-10 — Verify shutdown claims against the harness, not just `ps`
- **Mistake:** Told the user "nothing is running" while a background watcher shell (an `until` loop whose exit condition had become impossible after I killed the app) was still alive. My `ps` grep surfaced it as `/bin/zsh -c` and I dismissed it as my own command.
- **Rule:** Before claiming all background work is stopped: (1) stop every background task ID I started via TaskStop, not just pkill by name; (2) any `until`-loop watcher must be explicitly stopped when the thing it waits for is killed — its condition may never become true; (3) never hand-wave an unexplained process as "probably mine."

## 2026-06-09 — AI provider assumptions
- **Mistake:** Built M4 Anthropic-only even though the spec's AI layer listed OpenRouter as an option. The user doesn't have an Anthropic API key.
- **Rule:** Don't silently narrow a spec'd option list ("X or Y" → "X"). If cutting scope, say so at plan time and confirm the user can actually use what ships.

## 2026-06-09 — Keychain in dev builds
- **Mistake:** Used macOS Keychain for API keys in dev. Every `tauri dev` rebuild changes the binary signature, so macOS re-prompts for the login password constantly.
- **Rule:** Keychain (or any signature-bound secure store) is for release builds. In `debug_assertions` builds, store secrets in a plain file under the app data dir and say so in the UI.
