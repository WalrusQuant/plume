import { defineConfig } from "vitest/config";
import { fileURLToPath } from "node:url";

// Separate config from vite.config.js: the main config carries Tauri-specific
// server settings and the sveltekit plugin (for SSR/ runes compilation). These
// tests target pure TS logic — no Svelte, no Tauri, no DOM — so we only need
// $lib alias resolution.
export default defineConfig({
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL("./src/lib/", import.meta.url)),
    },
  },
  test: {
    include: ["src/**/*.test.ts"],
  },
});
