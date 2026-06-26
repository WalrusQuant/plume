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

  // Focus trap: tabbing past the last focusable element wraps to the first.
  const FOCUSABLE =
    'a[href], button:not([disabled]), textarea, input, select, [tabindex]:not([tabindex="-1"])';

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && dismissible) {
      e.preventDefault();
      handleOverlayClick();
      return;
    }
    if (e.key !== "Tab" || !panel) return;
    const nodes = Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
      (el) => el.offsetParent !== null,
    );
    if (nodes.length === 0) return;
    const first = nodes[0];
    const last = nodes[nodes.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }

  $effect(() => {
    if (!open) return;
    // capture the trigger so we can restore focus on close
    previouslyFocused = document.activeElement as HTMLElement | null;
    // move focus into the dialog (first focusable or the panel itself)
    queueMicrotask(() => {
      if (!panel) return;
      const first = panel.querySelector<HTMLElement>(FOCUSABLE);
      (first ?? panel).focus();
    });
    return () => {
      previouslyFocused?.focus?.();
      previouslyFocused = null;
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

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
