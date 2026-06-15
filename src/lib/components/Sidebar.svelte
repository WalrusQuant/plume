<script lang="ts">
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { api, type DocType, type Document, type Folder, type SearchHit } from "$lib/api";
  import { buildSidebarTree } from "$lib/buildSidebarTree";
  import { formatDate } from "$lib/formatDate";
  import { DOCUMENT_TYPES } from "$lib/documentTypes";
  import { MULTIPLY_TARGETS } from "$lib/multiplyTargets";
  import DocumentIcon from "$lib/components/DocumentIcon.svelte";
  import MoveToFolderMenu from "$lib/components/MoveToFolderMenu.svelte";

  interface Props {
    documents: Document[];
    folders: Folder[];
    selectedDocId: string | null;
    expandingId: string | null;
    expandingLabel: string;
    onSelect: (id: string) => void;
    onGoHome: () => void;
    onNewDocument: () => void;
    onNewIdea: () => void;
    onOpenIdea: (id: string) => void;
    onExpandIdea: (id: string, type: DocType, label: string) => void;
    onCancelExpand: () => void;
    onConvertIdea: (id: string, type: DocType) => void;
    onRename: (id: string, name: string) => void;
    onDelete: (id: string) => void;
    onMoveDocument: (id: string, folderId: string | null) => void;
    onReorderDocuments: (ids: string[]) => void;
    onReorderFolders: (ids: string[]) => void;
    onCreateFolder: (name: string) => Promise<Folder>;
    onRenameFolder: (id: string, name: string) => void;
    onDeleteFolder: (id: string) => void;
  }

  let {
    documents,
    folders,
    selectedDocId,
    expandingId,
    expandingLabel,
    onSelect,
    onGoHome,
    onNewDocument,
    onNewIdea,
    onOpenIdea,
    onExpandIdea,
    onCancelExpand,
    onConvertIdea,
    onRename,
    onDelete,
    onMoveDocument,
    onReorderDocuments,
    onReorderFolders,
    onCreateFolder,
    onRenameFolder,
    onDeleteFolder,
  }: Props = $props();

  /** Doc types an idea can be expanded into (label passed to the AI prompt).
      Shared with the document-multiply picker. */
  const EXPAND_TARGETS = MULTIPLY_TARGETS;
  /** Idea → document types it can be converted to as-is (no AI). All non-idea
      types qualify. */
  const CONVERT_TARGETS = DOCUMENT_TYPES;
  let expandMenuId = $state<string | null>(null);
  let convertMenuId = $state<string | null>(null);

  function toggleExpandMenu(id: string) {
    convertMenuId = null;
    expandMenuId = expandMenuId === id ? null : id;
  }

  function toggleConvertMenu(id: string) {
    expandMenuId = null;
    convertMenuId = convertMenuId === id ? null : id;
  }

  let editingId = $state<string | null>(null);
  let editName = $state("");
  let collapsedFolders = $state(new Set<string>());
  let moveDocId = $state<string | null>(null);
  let editingFolderId = $state<string | null>(null);
  let editFolderName = $state("");

  const tree = $derived(buildSidebarTree(folders, documents));
  const moveDoc = $derived(documents.find((d) => d.id === moveDocId));

  // ----- drag-and-drop reordering -----
  //
  // Reorder-only, and only within a row's own section. A section is one
  // contiguous orderable list: a folder's docs (keyed by folder id), the
  // unfiled docs ("unfiled"), the Inbox ideas ("inbox"), or the folders
  // themselves ("folders"). Cross-section moves stay on MoveToFolderMenu —
  // dragover only accepts (preventDefault) when kind + section match the drag
  // source, so foreign targets show the no-drop cursor and can't receive a drop.
  type DragKind = "doc" | "folder";
  let dragSource = $state<{ kind: DragKind; id: string; section: string } | null>(null);
  let dropTarget = $state<{ id: string; edge: "before" | "after" } | null>(null);

  /** The ordered id list of a section, read from the already-sorted tree. */
  function sectionIdsFor(section: string): string[] {
    if (section === "inbox") return tree.ideas.map((d) => d.id);
    if (section === "unfiled") return tree.unfiled.map((d) => d.id);
    if (section === "folders") return tree.folderTree.map((f) => f.id);
    const folder = tree.folderTree.find((f) => f.id === section);
    return folder ? folder.documents.map((d) => d.id) : [];
  }

  function handleDragStart(e: DragEvent, kind: DragKind, id: string, section: string) {
    // never start a drag from a row that's mid-rename (draggable is already off,
    // but guard anyway)
    if (kind === "doc" && editingId === id) return;
    if (kind === "folder" && editingFolderId === id) return;
    dragSource = { kind, id, section };
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      // WebKit refuses to begin a drag unless some data is attached
      e.dataTransfer.setData("text/plain", id);
    }
  }

  function handleDragOver(e: DragEvent, kind: DragKind, id: string, section: string) {
    if (!dragSource || dragSource.kind !== kind || dragSource.section !== section) return;
    if (dragSource.id === id) {
      dropTarget = null;
      return;
    }
    e.preventDefault(); // accept the drop (omitting this = no-drop cursor)
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move"; // move arrow, not copy badge
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const edge = e.clientY < rect.top + rect.height / 2 ? "before" : "after";
    dropTarget = { id, edge };
  }

  function handleDrop(e: DragEvent, kind: DragKind, id: string, section: string) {
    e.preventDefault();
    const source = dragSource;
    const edge = dropTarget?.edge ?? "before";
    // clear here: the reorder re-renders and may recycle this node before
    // dragend would fire
    dragSource = null;
    dropTarget = null;
    if (!source || source.kind !== kind || source.section !== section) return;
    if (source.id === id) return;

    const ids = sectionIdsFor(section);
    const from = ids.indexOf(source.id);
    if (from === -1) return;
    ids.splice(from, 1);
    let to = ids.indexOf(id);
    if (to === -1) return;
    if (edge === "after") to += 1;
    ids.splice(to, 0, source.id);

    if (kind === "folder") onReorderFolders(ids);
    else onReorderDocuments(ids);
  }

  function handleDragEnd() {
    dragSource = null;
    dropTarget = null;
  }

  // Cross-document full-text search. Non-empty query replaces the tree with
  // ranked results; debounced, with a sequence guard so out-of-order responses
  // can't overwrite newer ones.
  let searchQuery = $state("");
  let searchResults = $state<SearchHit[]>([]);
  let searchSeq = 0;
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  const searching = $derived(searchQuery.trim().length > 0);

  $effect(() => {
    const q = searchQuery.trim();
    clearTimeout(searchTimer);
    if (!q) {
      searchResults = [];
      return;
    }
    const seq = ++searchSeq;
    searchTimer = setTimeout(async () => {
      try {
        const hits = await api.searchDocuments(q);
        if (seq === searchSeq) searchResults = hits;
      } catch {
        if (seq === searchSeq) searchResults = [];
      }
    }, 150);
    return () => clearTimeout(searchTimer);
  });

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
    node.select();
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

{#snippet docItem(doc: Document, section: string)}
  <div
    class="sidebar-item {doc.id === selectedDocId ? 'sidebar-item--active' : ''}"
    class:sidebar-item--dragging={dragSource?.id === doc.id}
    class:sidebar-item--drop-before={dropTarget?.id === doc.id && dropTarget.edge === "before"}
    class:sidebar-item--drop-after={dropTarget?.id === doc.id && dropTarget.edge === "after"}
    draggable={editingId !== doc.id}
    ondragstart={(e) => handleDragStart(e, "doc", doc.id, section)}
    ondragover={(e) => handleDragOver(e, "doc", doc.id, section)}
    ondrop={(e) => handleDrop(e, "doc", doc.id, section)}
    ondragend={handleDragEnd}
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

{#snippet ideaItem(doc: Document)}
  <div
    class="sidebar-item {doc.id === selectedDocId ? 'sidebar-item--active' : ''}"
    class:sidebar-item--dragging={dragSource?.id === doc.id}
    class:sidebar-item--drop-before={dropTarget?.id === doc.id && dropTarget.edge === "before"}
    class:sidebar-item--drop-after={dropTarget?.id === doc.id && dropTarget.edge === "after"}
    draggable="true"
    ondragstart={(e) => handleDragStart(e, "doc", doc.id, "inbox")}
    ondragover={(e) => handleDragOver(e, "doc", doc.id, "inbox")}
    ondrop={(e) => handleDrop(e, "doc", doc.id, "inbox")}
    ondragend={handleDragEnd}
    onclick={() => onOpenIdea(doc.id)}
    onkeydown={(e) => e.key === "Enter" && onOpenIdea(doc.id)}
    role="button"
    tabindex="0"
  >
    <div class="sidebar-item-icon">
      <DocumentIcon type={doc.type} size={16} />
    </div>
    <div class="sidebar-item-info">
      <span class="sidebar-item-name">{doc.name}</span>
      {#if expandingId === doc.id}
        <span class="sidebar-idea-expanding">Expanding into {expandingLabel}…</span>
      {:else}
        <span class="sidebar-item-date">{formatDate(doc.updatedAt)}</span>
      {/if}
    </div>
    {#if expandingId === doc.id}
      <button
        class="sidebar-idea-cancel"
        onclick={(e) => {
          e.stopPropagation();
          onCancelExpand();
        }}
        title="Cancel expansion"
      >
        <span class="sidebar-idea-spinner"></span>
        <svg class="sidebar-idea-cancel-x" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    {:else}
      <div class="sidebar-item-actions">
          <button
            class="sidebar-action-btn"
            onclick={(e) => {
              e.stopPropagation();
              toggleExpandMenu(doc.id);
            }}
            title="Expand with AI"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M5 3v4M3 5h4M6 17v4M4 19h4M13 3l2.5 6.5L22 12l-6.5 2.5L13 21l-2.5-6.5L4 12l6.5-2.5L13 3z" />
            </svg>
          </button>
          <button
            class="sidebar-action-btn"
            onclick={(e) => {
              e.stopPropagation();
              toggleConvertMenu(doc.id);
            }}
            title="Convert to document"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M5 12h14M13 6l6 6-6 6" />
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
  {#if expandMenuId === doc.id}
    <div class="sidebar-expand-menu">
      <span class="sidebar-expand-menu-label">Expand into…</span>
      {#each EXPAND_TARGETS as target (target.type)}
        <button
          class="sidebar-expand-menu-item"
          onclick={() => {
            expandMenuId = null;
            onExpandIdea(doc.id, target.type, target.label);
          }}
        >
          <DocumentIcon type={target.type} size={14} />
          {target.label}
        </button>
      {/each}
    </div>
  {/if}
  {#if convertMenuId === doc.id}
    <div class="sidebar-expand-menu">
      <span class="sidebar-expand-menu-label">Convert to…</span>
      {#each CONVERT_TARGETS as target (target.type)}
        <button
          class="sidebar-expand-menu-item"
          onclick={() => {
            convertMenuId = null;
            onConvertIdea(doc.id, target.type);
          }}
        >
          <DocumentIcon type={target.type} size={14} />
          {target.label}
        </button>
      {/each}
    </div>
  {/if}
{/snippet}

<aside class="sidebar">
  <div class="sidebar-brand">
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <polyline points="4 17 10 11 4 5" />
      <line x1="12" y1="19" x2="20" y2="19" />
    </svg>
    <span class="sidebar-brand-text">Plume</span>
    <button class="sidebar-home-btn" onclick={onGoHome} title="Home">
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
        <polyline points="9 22 9 12 15 12 15 22" />
      </svg>
    </button>
  </div>

  <div class="sidebar-search">
    <svg class="sidebar-search-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="11" cy="11" r="7" />
      <line x1="21" y1="21" x2="16.65" y2="16.65" />
    </svg>
    <input
      class="sidebar-search-input"
      type="text"
      placeholder="Search documents…"
      bind:value={searchQuery}
    />
    {#if searching}
      <button class="sidebar-search-clear" onclick={() => (searchQuery = "")} title="Clear search">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    {/if}
  </div>

  {#if searching}
    <nav class="sidebar-search-results">
      {#each searchResults as hit (hit.id)}
        <button class="sidebar-search-result" onclick={() => onSelect(hit.id)}>
          <span class="sidebar-search-result-head">
            <DocumentIcon type={hit.type} size={14} />
            <span class="sidebar-search-result-name">{hit.name}</span>
          </span>
          {#if hit.snippet}
            <span class="sidebar-search-result-snippet">{hit.snippet}</span>
          {/if}
        </button>
      {/each}
      {#if searchResults.length === 0}
        <div class="sidebar-search-empty">No matches</div>
      {/if}
    </nav>
  {:else}
  <div class="sidebar-section-header">
    <span class="sidebar-section-label">Inbox</span>
    <div class="sidebar-section-actions">
      <button class="sidebar-new-btn" onclick={onNewIdea} title="New idea">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
          <line x1="12" y1="5" x2="12" y2="19" />
          <line x1="5" y1="12" x2="19" y2="12" />
        </svg>
      </button>
    </div>
  </div>
  <nav class="sidebar-inbox">
    {#each tree.ideas as idea (idea.id)}
      {@render ideaItem(idea)}
    {/each}
    {#if tree.ideas.length === 0}
      <button class="sidebar-inbox-empty" onclick={onNewIdea}>
        Capture a quick idea…
      </button>
    {/if}
  </nav>

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
          class:sidebar-folder-header--dragging={dragSource?.id === folder.id}
          class:sidebar-folder-header--drop-before={dropTarget?.id === folder.id && dropTarget.edge === "before"}
          class:sidebar-folder-header--drop-after={dropTarget?.id === folder.id && dropTarget.edge === "after"}
          draggable={editingFolderId !== folder.id}
          ondragstart={(e) => handleDragStart(e, "folder", folder.id, "folders")}
          ondragover={(e) => handleDragOver(e, "folder", folder.id, "folders")}
          ondrop={(e) => handleDrop(e, "folder", folder.id, "folders")}
          ondragend={handleDragEnd}
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
              {@render docItem(doc, folder.id)}
            {/each}
            {#if folder.documents.length === 0}
              <div class="sidebar-folder-empty">Empty folder</div>
            {/if}
          </div>
        {/if}
      </div>
    {/each}

    {#each tree.unfiled as doc (doc.id)}
      {@render docItem(doc, "unfiled")}
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
  {/if}

  {#if moveDocId && moveDoc}
    <MoveToFolderMenu
      {folders}
      currentFolderId={moveDoc.folderId}
      onMove={(folderId) => onMoveDocument(moveDocId!, folderId)}
      onClose={() => (moveDocId = null)}
    />
  {/if}
</aside>

<style>
  .sidebar-search {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin: 0 0.5rem 0.5rem;
    padding: 0.4rem 0.55rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-secondary);
  }
  .sidebar-search:focus-within {
    border-color: var(--accent);
  }
  .sidebar-search-icon {
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .sidebar-search-input {
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-size: 0.85rem;
    outline: none;
  }
  .sidebar-search-clear {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    flex-shrink: 0;
  }
  .sidebar-search-clear:hover {
    color: var(--text-primary);
  }

  .sidebar-search-results {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding: 0 0.5rem;
    overflow-y: auto;
  }
  .sidebar-search-result {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    width: 100%;
    padding: 0.45rem 0.55rem;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
  }
  .sidebar-search-result:hover {
    background: var(--bg-secondary);
  }
  .sidebar-search-result-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .sidebar-search-result-name {
    font-size: 0.85rem;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sidebar-search-result-snippet {
    font-size: 0.75rem;
    color: var(--text-secondary);
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .sidebar-search-empty {
    padding: 0.6rem 0.55rem;
    font-size: 0.8rem;
    color: var(--text-secondary);
  }
</style>
