# Lessons

## 2026-06-09 — AI provider assumptions
- **Mistake:** Built M4 Anthropic-only even though the spec's AI layer listed OpenRouter as an option. The user doesn't have an Anthropic API key.
- **Rule:** Don't silently narrow a spec'd option list ("X or Y" → "X"). If cutting scope, say so at plan time and confirm the user can actually use what ships.

## 2026-06-09 — Keychain in dev builds
- **Mistake:** Used macOS Keychain for API keys in dev. Every `tauri dev` rebuild changes the binary signature, so macOS re-prompts for the login password constantly.
- **Rule:** Keychain (or any signature-bound secure store) is for release builds. In `debug_assertions` builds, store secrets in a plain file under the app data dir and say so in the UI.
