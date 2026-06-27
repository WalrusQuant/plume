<script lang="ts">
  import type { Folder } from "$lib/api";

  interface Props {
    folders: Folder[];
    currentFolderId: string | null;
    onMove: (folderId: string | null) => void;
    onClose: () => void;
  }

  let { folders, currentFolderId, onMove, onClose }: Props = $props();

  let menuEl: HTMLDivElement | null = $state(null);
  let previouslyFocused: HTMLElement | null = null;

  const FOCUSABLE = 'button:not([disabled]), [tabindex="0"]';

  function focusableItems(): HTMLElement[] {
    if (!menuEl) return [];
    return Array.from(menuEl.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
      (el) => el.offsetParent !== null,
    );
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
      return;
    }
    const items = focusableItems();
    if (items.length === 0) return;
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      items[(i + 1) % items.length].focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      items[(i - 1 + items.length) % items.length].focus();
    } else if (e.key === "Home") {
      e.preventDefault();
      items[0].focus();
    } else if (e.key === "End") {
      e.preventDefault();
      items[items.length - 1].focus();
    } else if (e.key === "Tab") {
      // trap Tab within the menu while open
      e.preventDefault();
      if (e.shiftKey) items[(i - 1 + items.length) % items.length].focus();
      else items[(i + 1) % items.length].focus();
    }
  }

  $effect(() => {
    previouslyFocused = document.activeElement as HTMLElement | null;
    queueMicrotask(() => {
      const first = menuEl?.querySelector<HTMLElement>(FOCUSABLE);
      first?.focus();
    });
    return () => {
      previouslyFocused?.focus?.();
      previouslyFocused = null;
    };
  });

  function move(folderId: string | null) {
    onMove(folderId);
    onClose();
  }
</script>

<div class="move-menu-overlay" onclick={onClose} role="presentation">
  <div
    bind:this={menuEl}
    class="move-menu"
    onclick={(e) => e.stopPropagation()}
    onkeydown={onKeydown}
    role="menu"
    aria-label="Move to folder"
    tabindex="-1"
  >
    <div class="move-menu-header">Move to folder</div>
    {#if currentFolderId}
      <button class="move-menu-item" role="menuitem" onclick={() => move(null)}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
        Remove from folder
      </button>
    {/if}
    {#each folders as folder (folder.id)}
      <button
        class="move-menu-item {folder.id === currentFolderId ? 'move-menu-item--current' : ''}"
        role="menuitem"
        onclick={() => move(folder.id)}
        disabled={folder.id === currentFolderId}
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
        </svg>
        {folder.name}
      </button>
    {/each}
    {#if folders.length === 0}
      <div class="move-menu-empty">No folders yet</div>
    {/if}
  </div>
</div>
