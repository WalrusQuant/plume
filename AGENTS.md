# Repository Guidelines

## Project Structure & Module Organization

Plume is a local-first Tauri 2 desktop application. The SvelteKit/Svelte 5 frontend lives in `src/`: routes are in `src/routes`, reusable UI in `src/lib/components`, pure TypeScript helpers in `src/lib`, and global theme variables in `src/app.css`. Static files belong in `static/`; documentation assets belong in `docs/`.

The Rust backend is under `src-tauri/src`. Tauri command wrappers live in `commands.rs`, SQLite persistence and append-only migrations in `storage.rs`, AI integrations in `ai.rs`, and target-specific renderers in `export/`. Keep frontend `invoke()` wrappers in `src/lib/api.ts` aligned with Rust commands.

## Build, Test, and Development Commands

Prerequisites are Rust 1.95+, Node 22+, and pnpm.

- `pnpm install` installs JavaScript dependencies.
- `pnpm tauri dev` runs the desktop app with frontend hot reload.
- `pnpm check` runs Svelte and TypeScript diagnostics.
- `pnpm test` runs the Vitest suite once; `pnpm test:watch` runs it interactively.
- `cargo test --manifest-path src-tauri/Cargo.toml` runs Rust unit tests.
- `pnpm tauri build` creates a production desktop bundle.

## Coding Style & Naming Conventions

Use two-space indentation in TypeScript, Svelte, JSON, and CSS; let `rustfmt` format Rust. TypeScript is strict. Name Svelte components in `PascalCase`, helpers and variables in `camelCase`, and Rust modules/functions in `snake_case`. Use Svelte 5 runes and CSS custom properties; do not introduce Tailwind. Keep Tauri wrappers thin and database functions testable over `&Connection`. Never modify a shipped migration—append a new one.

## Testing Guidelines

Place pure frontend tests beside their modules as `*.test.ts`; Vitest discovers `src/**/*.test.ts`. Put Rust tests in local `#[cfg(test)] mod tests` blocks. Add focused regression tests for storage, parsing, imports, AI request construction, and export rendering. There is no numeric coverage gate, but changed behavior should be exercised. Before opening a PR, run `pnpm check`, `pnpm test`, and the Rust suite.

## Commit & Pull Request Guidelines

Follow the repository’s Conventional Commit style: `feat:`, `fix:`, or `docs:` followed by a concise imperative summary. Keep commits scoped to one logical change. PRs should explain the user-visible result, implementation notes, and verification performed; link related issues and include screenshots or recordings for UI changes. Do not commit API keys, `dev-keys.json`, application databases, or downloaded embedding models.
