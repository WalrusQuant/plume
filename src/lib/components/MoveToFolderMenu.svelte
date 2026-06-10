<script lang="ts">
  import type { Folder } from "$lib/api";

  interface Props {
    folders: Folder[];
    currentFolderId: string | null;
    onMove: (folderId: string | null) => void;
    onClose: () => void;
  }

  let { folders, currentFolderId, onMove, onClose }: Props = $props();
</script>

<div class="move-menu-overlay" onclick={onClose} role="presentation">
  <div
    class="move-menu"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.key === "Escape" && onClose()}
    role="menu"
    tabindex="-1"
  >
    <div class="move-menu-header">Move to folder</div>
    {#if currentFolderId}
      <button
        class="move-menu-item"
        onclick={() => {
          onMove(null);
          onClose();
        }}
      >
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
        onclick={() => {
          onMove(folder.id);
          onClose();
        }}
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
