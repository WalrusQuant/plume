/**
 * Format an unknown catch value into a readable string. Tauri IPC errors
 * serialize as plain strings (see error.rs), native JS errors have `.message`,
 * and everything else falls back to `String(e)`. Avoids `[object Object]` in
 * toast messages.
 */
export function formatError(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  return String(e);
}
