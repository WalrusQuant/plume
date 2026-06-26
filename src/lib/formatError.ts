/**
 * Format an unknown catch value into a readable string.
 *
 * Three shapes:
 * - `{ kind, message }` — Rust `Error` crossing Tauri IPC (see error.rs)
 * - `Error` — native JS exception (`.message`)
 * - `string` — already a message
 *
 * Falls back to `String(e)` for anything else. Avoids `[object Object]`.
 */
export function formatError(e: unknown): string {
  if (e !== null && typeof e === "object" && "message" in e && "kind" in e) {
    const msg = (e as { message: unknown }).message;
    return typeof msg === "string" ? msg : String(msg);
  }
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  return String(e);
}

/**
 * Extract the typed `kind` from a Rust `Error` crossing Tauri IPC, or `null`
 * for any other error shape. Useful when a caller wants to branch on
 * `not_found` vs `invalid_input` etc.
 */
export function errorKind(e: unknown): string | null {
  if (e !== null && typeof e === "object" && "kind" in e && "message" in e) {
    const k = (e as { kind: unknown }).kind;
    return typeof k === "string" ? k : null;
  }
  return null;
}
