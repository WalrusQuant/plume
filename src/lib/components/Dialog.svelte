<script lang="ts">
  // Accessible modal dialog shell: focus trap, aria-modal, Escape/overlay
  // dismissal, focus restoration to the trigger on close. Wraps the shared
  // `.dialog-*` CSS classes already used across the app.

  interface Props {
    open: boolean;
    title: string;
    onClose: () => void;
    /** Override the overlay-click behavior (defaults to onClose). Set to a
        no-op when a dialog guards unsaved edits (e.g. IdeaCaptureModal). */
    onOverlayClick?: () => void;
    /** Disable dismissal entirely (e.g. MultiplyModal while a run is active). */
    dismissible?: boolean;
    children: import("svelte").Snippet;
    footer?: import("svelte").Snippet;
  }

  let {
    open,
    title,
    onClose,
    onOverlayClick,
    dismissible = true,
    children,
    footer,
  }: Props = $props();

  const handleOverlayClick = () => (onOverlayClick ?? onClose)();
  const titleId = `dialog-title-${Math.random().toString(36).slice(2, 9)}`;

  let panel: HTMLDivElement | null = $state(null);
  let previouslyFocused: HTMLElement | null = null;

  // Focus trap: only intercept Tab when focus would escape the dialog
  // (i.e., the active element is the first/last focusable). This lets
  // textareas and inputs handle Tab natively — inserting a tab character or
  // moving between fields — instead of being trapped.
  const FOCUSABLE =
    'a[href], button:not([disabled]), textarea, input, select, [tabindex]:not([tabindex="-1"])';

  function onKeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape" && dismissible) {
      e.preventDefault();
      handleOverlayClick();
      return;
    }
    if (e.key !== "Tab") return;
    const active = document.activeElement;
    // If focus is in a text-editable field that isn't at the dialog boundary,
    // let the browser handle Tab natively (textarea tab insertion, field-to-field
    // navigation, etc.)
    const isEditable =
      active instanceof HTMLTextAreaElement ||
      active instanceof HTMLInputElement ||
      active instanceof HTMLSelectElement;
    const nodes = panel
      ? Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
          (el) => el.offsetParent !== null,
        )
      : [];
    if (nodes.length === 0) return;
    const first = nodes[0];
    const last = nodes[nodes.length - 1];
    const atFirst = active === first;
    const atLast = active === last;
    // Only intercept at the wrap points; everything else uses native Tab
    if (e.shiftKey && atFirst) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && atLast) {
      e.preventDefault();
      first.focus();
    } else if (isEditable) {
      // editable field in the middle of the dialog — let the browser handle it
    }
  }

  $effect(() => {
    if (!open) return;
    // capture the trigger so we can restore focus on close
    previouslyFocused = document.activeElement as HTMLElement | null;
    // lock background scroll while the modal is open
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    // move focus into the dialog (first focusable or the panel itself)
    queueMicrotask(() => {
      if (!panel) return;
      const first = panel.querySelector<HTMLElement>(FOCUSABLE);
      (first ?? panel).focus();
    });
    // Only attach the keydown listener while the dialog is open — avoids
    // interfering with the main editor's Tab handling when no dialog is visible.
    window.addEventListener("keydown", onKeydown);
    return () => {
      window.removeEventListener("keydown", onKeydown);
      document.body.style.overflow = prevOverflow;
      previouslyFocused?.focus?.();
      previouslyFocused = null;
    };
  });
</script>

{#if open}
  <div
    class="dialog-overlay"
    onclick={handleOverlayClick}
    onkeydown={(e) => {
      if (e.key === "Escape" && dismissible) handleOverlayClick();
    }}
    role="presentation"
  >
    <div
      bind:this={panel}
      class="dialog"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      tabindex="-1"
    >
      <div class="dialog-header">
        <h3 class="dialog-title" id={titleId}>{title}</h3>
        {#if dismissible}
          <button class="dialog-close" onclick={handleOverlayClick} title="Close" aria-label="Close">
            <svg
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        {/if}
      </div>

      {@render children()}
      {#if footer}{@render footer()}{/if}
    </div>
  </div>
{/if}
