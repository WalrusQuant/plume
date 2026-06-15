/** Svelte action: invoke `handler` on a pointerdown outside `node` or on Escape.
    Attach to the container that wraps BOTH the trigger and the menu, so a click
    on the trigger (which toggles the menu) isn't mistaken for an outside click.
    The handler should be idempotent (e.g. set `open = false`). */
export function clickOutside(node: HTMLElement, handler: () => void) {
  function onPointerDown(e: PointerEvent) {
    if (!node.contains(e.target as Node)) handler();
  }
  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") handler();
  }
  // capture so we see the click before it can be stopped by inner handlers
  document.addEventListener("pointerdown", onPointerDown, true);
  document.addEventListener("keydown", onKeyDown);
  return {
    destroy() {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown);
    },
  };
}
