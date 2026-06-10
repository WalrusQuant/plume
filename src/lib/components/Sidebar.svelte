<script lang="ts">
  import { confirm } from "@tauri-apps/plugin-dialog";
  import type { Document, Folder } from "$lib/api";
  import { buildSidebarTree } from "$lib/buildSidebarTree";
  import DocumentIcon from "$lib/components/DocumentIcon.svelte";
  import MoveToFolderMenu from "$lib/components/MoveToFolderMenu.svelte";

  interface Props {
    documents: Document[];
    folders: Folder[];
    selectedDocId: string | null;
    onSelect: (id: string) => void;
    onNewDocument: () => void;
    onRename: (id: string, name: string) => void;
    onDelete: (id: string) => void;
    onMoveDocument: (id: string, folderId: string | null) => void;
    onCreateFolder: (name: string) => Promise<Folder>;
    onRenameFolder: (id: string, name: string) => void;
    onDeleteFolder: (id: string) => void;
  }

  let {
    documents,
    folders,
    selectedDocId,
    onSelect,
    onNewDocument,
    onRename,
    onDelete,
    onMoveDocument,
    onCreateFolder,
    onRenameFolder,
    onDeleteFolder,
  }: Props = $props();

  let editingId = $state<string | null>(null);
  let editName = $state("");
  let collapsedFolders = $state(new Set<string>());
  let moveDocId = $state<string | null>(null);
  let editingFolderId = $state<string | null>(null);
  let editFolderName = $state("");

  const tree = $derived(buildSidebarTree(folders, documents));
  const moveDoc = $derived(documents.find((d) => d.id === moveDocId));

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  function formatDate(iso: string): string {
    const d = new Date(iso);
    const now = new Date();
    const diffMins = Math.floor((now.getTime() - d.getTime()) / 60000);
    if (diffMins < 1) return "Just now";
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    const diffDays = Math.floor(diffHours / 24);
    if (diffDays < 7) return `${diffDays}d ago`;
    return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
  }

  function startRename(doc: Document) {
    editingId = doc.id;
    editName = doc.name;
  }

  function commitRename() {
    if (editingId && editName.trim()) {
      onRename(editingId, editName.trim());
    }
    editingId = null;
  }

  async function handleDelete(id: string, name: string) {
    if (await confirm(`Delete "${name}"?`, { kind: "warning" })) {
      onDelete(id);
    }
  }

  function toggleFolder(folderId: string) {
    const next = new Set(collapsedFolders);
    if (next.has(folderId)) next.delete(folderId);
    else next.add(folderId);
    collapsedFolders = next;
  }

  async function handleNewFolder() {
    const folder = await onCreateFolder("New Folder");
    editingFolderId = folder.id;
    editFolderName = folder.name;
  }

  function startFolderRename(folder: Folder) {
    editingFolderId = folder.id;
    editFolderName = folder.name;
  }

  function commitFolderRename() {
    if (editingFolderId && editFolderName.trim()) {
      onRenameFolder(editingFolderId, editFolderName.trim());
    }
    editingFolderId = null;
  }

  async function handleDeleteFolder(folder: Folder) {
    const docsInFolder = documents.filter((d) => d.folderId === folder.id);
    const msg =
      docsInFolder.length > 0
        ? `Delete folder "${folder.name}"? ${docsInFolder.length} document(s) will be moved to unfiled.`
        : `Delete folder "${folder.name}"?`;
    if (await confirm(msg, { kind: "warning" })) {
      // backend FK is ON DELETE SET NULL; the page updates the doc list to match
      onDeleteFolder(folder.id);
    }
  }
</script>

{#snippet docItem(doc: Document)}
  <div
    class="sidebar-item {doc.id === selectedDocId ? 'sidebar-item--active' : ''}"
    onclick={() => onSelect(doc.id)}
    onkeydown={(e) => e.key === "Enter" && onSelect(doc.id)}
    role="button"
    tabindex="0"
  >
    {#if editingId === doc.id}
      <input
        class="sidebar-rename-input"
        bind:value={editName}
        onblur={commitRename}
        onkeydown={(e) => {
          if (e.key === "Enter") commitRename();
          if (e.key === "Escape") editingId = null;
        }}
        onclick={(e) => e.stopPropagation()}
        use:focusOnMount
      />
    {:else}
      <div class="sidebar-item-icon">
        <DocumentIcon type={doc.type} size={16} />
      </div>
      <div class="sidebar-item-info">
        <span class="sidebar-item-name">{doc.name}</span>
        <span class="sidebar-item-date">{formatDate(doc.updatedAt)}</span>
      </div>
      <div class="sidebar-item-actions">
        <button
          class="sidebar-action-btn"
          onclick={(e) => {
            e.stopPropagation();
            moveDocId = doc.id;
          }}
          title="Move to folder"
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
          </svg>
        </button>
        <button
          class="sidebar-action-btn"
          onclick={(e) => {
            e.stopPropagation();
            startRename(doc);
          }}
          title="Rename"
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
          </svg>
        </button>
        <button
          class="sidebar-action-btn sidebar-action-btn--delete"
          onclick={(e) => {
            e.stopPropagation();
            void handleDelete(doc.id, doc.name);
          }}
          title="Delete"
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 6h18 M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6 M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
          </svg>
        </button>
      </div>
    {/if}
  </div>
{/snippet}

<aside class="sidebar">
  <div class="sidebar-brand">
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <polyline points="4 17 10 11 4 5" />
      <line x1="12" y1="19" x2="20" y2="19" />
    </svg>
    <span class="sidebar-brand-text">Plume</span>
  </div>

  <div class="sidebar-section-header">
    <span class="sidebar-section-label">Documents</span>
    <div class="sidebar-section-actions">
      <button class="sidebar-new-btn" onclick={() => void handleNewFolder()} title="New folder">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
        </svg>
      </button>
      <button class="sidebar-new-btn" onclick={onNewDocument} title="New document">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
          <line x1="12" y1="5" x2="12" y2="19" />
          <line x1="5" y1="12" x2="19" y2="12" />
        </svg>
      </button>
    </div>
  </div>

  <nav class="sidebar-list">
    {#each tree.folderTree as folder (folder.id)}
      <div class="sidebar-folder">
        <div
          class="sidebar-folder-header"
          onclick={() => toggleFolder(folder.id)}
          onkeydown={(e) => e.key === "Enter" && toggleFolder(folder.id)}
          role="button"
          tabindex="0"
        >
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            class="sidebar-folder-chevron {collapsedFolders.has(folder.id) ? '' : 'sidebar-folder-chevron--open'}"
          >
            <polyline points="9 18 15 12 9 6" />
          </svg>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
          </svg>
          {#if editingFolderId === folder.id}
            <input
              class="sidebar-rename-input"
              bind:value={editFolderName}
              onblur={commitFolderRename}
              onkeydown={(e) => {
                if (e.key === "Enter") commitFolderRename();
                if (e.key === "Escape") editingFolderId = null;
              }}
              onclick={(e) => e.stopPropagation()}
              use:focusOnMount
            />
          {:else}
            <span class="sidebar-folder-name">{folder.name}</span>
          {/if}
          <div class="sidebar-folder-actions">
            <button
              class="sidebar-action-btn"
              onclick={(e) => {
                e.stopPropagation();
                startFolderRename(folder);
              }}
              title="Rename folder"
            >
              <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
              </svg>
            </button>
            <button
              class="sidebar-action-btn sidebar-action-btn--delete"
              onclick={(e) => {
                e.stopPropagation();
                void handleDeleteFolder(folder);
              }}
              title="Delete folder"
            >
              <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M3 6h18 M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6 M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
              </svg>
            </button>
          </div>
        </div>
        {#if !collapsedFolders.has(folder.id)}
          <div class="sidebar-folder-children">
            {#each folder.documents as doc (doc.id)}
              {@render docItem(doc)}
            {/each}
            {#if folder.documents.length === 0}
              <div class="sidebar-folder-empty">Empty folder</div>
            {/if}
          </div>
        {/if}
      </div>
    {/each}

    {#each tree.unfiled as doc (doc.id)}
      {@render docItem(doc)}
    {/each}

    {#if documents.length === 0 && folders.length === 0}
      <div class="sidebar-empty">
        <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" opacity="0.4">
          <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
          <polyline points="14,2 14,8 20,8" />
        </svg>
        <p>No documents yet</p>
        <button class="sidebar-empty-btn" onclick={onNewDocument}>Create your first document</button>
      </div>
    {/if}
  </nav>

  {#if moveDocId && moveDoc}
    <MoveToFolderMenu
      {folders}
      currentFolderId={moveDoc.folderId}
      onMove={(folderId) => onMoveDocument(moveDocId!, folderId)}
      onClose={() => (moveDocId = null)}
    />
  {/if}
</aside>
