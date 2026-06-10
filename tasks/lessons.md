# Lessons

## 2026-06-10 — Verify shutdown claims against the harness, not just `ps`
- **Mistake:** Told the user "nothing is running" while a background watcher shell (an `until` loop whose exit condition had become impossible after I killed the app) was still alive. My `ps` grep surfaced it as `/bin/zsh -c` and I dismissed it as my own command.
- **Rule:** Before claiming all background work is stopped: (1) stop every background task ID I started via TaskStop, not just pkill by name; (2) any `until`-loop watcher must be explicitly stopped when the thing it waits for is killed — its condition may never become true; (3) never hand-wave an unexplained process as "probably mine."

## 2026-06-09 — AI provider assumptions
- **Mistake:** Built M4 Anthropic-only even though the spec's AI layer listed OpenRouter as an option. The user doesn't have an Anthropic API key.
- **Rule:** Don't silently narrow a spec'd option list ("X or Y" → "X"). If cutting scope, say so at plan time and confirm the user can actually use what ships.

## 2026-06-09 — Keychain in dev builds
- **Mistake:** Used macOS Keychain for API keys in dev. Every `tauri dev` rebuild changes the binary signature, so macOS re-prompts for the login password constantly.
- **Rule:** Keychain (or any signature-bound secure store) is for release builds. In `debug_assertions` builds, store secrets in a plain file under the app data dir and say so in the UI.
