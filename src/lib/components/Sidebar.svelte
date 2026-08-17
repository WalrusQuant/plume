<script lang="ts">
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { api, type DocType, type Document, type Folder, type SearchHit } from "$lib/api";
  import { buildSidebarTree } from "$lib/buildSidebarTree";
  import { formatDate } from "$lib/formatDate";
  import { shouldActivateFromKey } from "$lib/keyboardActivation";
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
    onImport: () => void;
    onNewIdea: () => void;
    onClearIdeas: () => void;
    onOpenIdea: (id: string) => void;
    onOpenSource: (id: string) => void;
    onRemoveSource: (id: string) => void;
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
    onImport,
    onNewIdea,
    onClearIdeas,
    onOpenIdea,
    onOpenSource,
    onRemoveSource,
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

  // Dismiss the Expand/Convert menus on any outside pointerdown or Escape.
  // The menu and its trigger button are siblings (not nested), so a plain
  // clickOutside action on a shared container won't work; use a document listener.
  $effect(() => {
    if (!expandMenuId && !convertMenuId) return;
    const onDown = (e: PointerEvent) => {
      const t = e.target as HTMLElement | null;
      // keep the click that lands on a menu or a trigger button (it toggles)
      if (t?.closest(".sidebar-expand-menu, .sidebar-action-btn")) return;
      expandMenuId = null;
      convertMenuId = null;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        expandMenuId = null;
        convertMenuId = null;
      }
    };
    document.addEventListener("pointerdown", onDown, true);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDown, true);
      document.removeEventListener("keydown", onKey);
    };
  });

  let editingId = $state<string | null>(null);
  let editName = $state("");
  let collapsedFolders = $state(new Set<string>());
  let moveDocId = $state<string | null>(null);
  let editingFolderId = $state<string | null>(null);
  let editFolderName = $state("");

  const tree = $derived(buildSidebarTree(folders, documents));
  const moveDoc = $derived(documents.find((d) => d.id === moveDocId));

  // ----- pointer reordering -----
  //
  // HTML5 drag-and-drop starts an OS drag. With Tauri's file-drop handler on,
  // macOS treats that as "copy a file into the window" (green plus + the
  // import overlay) and the in-app drop never fires. Pointer capture stays
  // inside the webview, so reorder doesn't collide with drop-to-import.
  //
  // Reorder within a section, or drop a document onto a folder header to move it.
  type DragKind = "doc" | "folder";
  const DRAG_THRESHOLD_PX = 5;
  let dragSource = $state<{ kind: DragKind; id: string; section: string } | null>(null);
  let dropTarget = $state<{ id: string; edge: "before" | "after" } | null>(null);
  let folderDropId = $state<string | null>(null);
  let suppressClick = false;

  /** The ordered id list of a section, read from the already-sorted tree. */
  function sectionIdsFor(section: string): string[] {
    if (section === "inbox") return tree.ideas.map((d) => d.id);
    if (section === "unfiled") return tree.unfiled.map((d) => d.id);
    if (section === "sources") return tree.sources.map((d) => d.id);
    if (section === "folders") return tree.folderTree.map((f) => f.id);
    const folder = tree.folderTree.find((f) => f.id === section);
    return folder ? folder.documents.map((d) => d.id) : [];
  }

  function isReorderFrom(target: EventTarget | null): boolean {
    if (!(target instanceof Element)) return false;
    return !target.closest("button, input, a, .sidebar-expand-menu");
  }

  function applyDrop(source: { kind: DragKind; id: string; section: string }, targetId: string, edge: "before" | "after") {
    if (source.id === targetId) return;
    const ids = sectionIdsFor(source.section);
    const from = ids.indexOf(source.id);
    if (from === -1) return;
    ids.splice(from, 1);
    let to = ids.indexOf(targetId);
    if (to === -1) return;
    if (edge === "after") to += 1;
    ids.splice(to, 0, source.id);
    if (source.kind === "folder") onReorderFolders(ids);
    else onReorderDocuments(ids);
  }

  function canDropIntoFolders(source: { kind: DragKind; section: string }): boolean {
    return source.kind === "doc" && source.section !== "inbox" && source.section !== "sources";
  }

  function updateDropTarget(clientX: number, clientY: number) {
    if (!dragSource) return;
    const el = document.elementFromPoint(clientX, clientY);
    if (canDropIntoFolders(dragSource)) {
      const folderHit = el?.closest<HTMLElement>("[data-drop-folder]");
      const into = folderHit?.dataset.dropFolder ?? null;
      if (into && into !== dragSource.section) {
        folderDropId = into;
        dropTarget = null;
        return;
      }
    }
    folderDropId = null;
    const hit = el?.closest<HTMLElement>("[data-reorder-id]");
    if (!hit) {
      dropTarget = null;
      return;
    }
    const kind = hit.dataset.reorderKind as DragKind | undefined;
    const id = hit.dataset.reorderId;
    const section = hit.dataset.reorderSection;
    if (!kind || !id || !section) {
      dropTarget = null;
      return;
    }
    if (kind !== dragSource.kind || section !== dragSource.section || id === dragSource.id) {
      dropTarget = null;
      return;
    }
    const rect = hit.getBoundingClientRect();
    dropTarget = { id, edge: clientY < rect.top + rect.height / 2 ? "before" : "after" };
  }

  function handlePointerDown(e: PointerEvent, kind: DragKind, id: string, section: string) {
    if (e.button !== 0) return;
    if (kind === "doc" && editingId === id) return;
    if (kind === "folder" && editingFolderId === id) return;
    if (!isReorderFrom(e.target)) return;

    const originX = e.clientX;
    const originY = e.clientY;
    let started = false;
    window.getSelection()?.removeAllRanges();

    const onMove = (ev: PointerEvent) => {
      ev.preventDefault();
      window.getSelection()?.removeAllRanges();
      if (!started) {
        const dx = ev.clientX - originX;
        const dy = ev.clientY - originY;
        if (dx * dx + dy * dy < DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX) return;
        started = true;
        suppressClick = true;
        dragSource = { kind, id, section };
      }
      updateDropTarget(ev.clientX, ev.clientY);
    };

    const stop = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      window.removeEventListener("keydown", onKey);
      const source = dragSource;
      const target = dropTarget;
      const intoFolder = folderDropId;
      dragSource = null;
      dropTarget = null;
      folderDropId = null;
      if (started && source && intoFolder && canDropIntoFolders(source)) {
        onMoveDocument(source.id, intoFolder);
      } else if (started && source && target) {
        applyDrop(source, target.id, target.edge);
      }
      if (started) ev.preventDefault();
      // click may not fire if the pointer was released on another row
      requestAnimationFrame(() => {
        suppressClick = false;
      });
    };

    const onKey = (ev: KeyboardEvent) => {
      if (ev.key !== "Escape") return;
      dragSource = null;
      dropTarget = null;
      folderDropId = null;
      started = false;
      suppressClick = false;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      window.removeEventListener("keydown", onKey);
    };

    window.addEventListener("pointermove", onMove, { passive: false });
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
    window.addEventListener("keydown", onKey);
  }

  // Cross-document full-text search. Non-empty query replaces the tree with
  // ranked results; debounced, with a sequence guard so out-of-order responses
  // can't overwrite newer ones.
  let searchQuery = $state("");
  let searchResults = $state<SearchHit[]>([]);
  let searchFailed = $state(false);
  let searchSeq = 0;
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  const searching = $derived(searchQuery.trim().length > 0);

  $effect(() => {
    const q = searchQuery.trim();
    clearTimeout(searchTimer);
    if (!q) {
      searchResults = [];
      searchFailed = false;
      return;
    }
    const seq = ++searchSeq;
    searchTimer = setTimeout(async () => {
      try {
        const hits = await api.searchDocuments(q);
        if (seq === searchSeq) {
          searchResults = hits;
          searchFailed = false;
        }
      } catch {
        // distinguish a real failure from a genuine no-match
        if (seq === searchSeq) {
          searchResults = [];
          searchFailed = true;
        }
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
    try {
      const folder = await onCreateFolder("New Folder");
      editingFolderId = folder.id;
      editFolderName = folder.name;
    } catch {
      // createFolder already surfaced a toast; just don't enter rename mode
    }
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

  /** Keyboard activation for clickable rows — Enter or Space (button parity). */
  function activateOn(e: KeyboardEvent, fn: () => void) {
    if (shouldActivateFromKey(e)) {
      e.preventDefault();
      fn();
    }
  }

  async function handleDeleteFolder(folder: Folder) {    const docsInFolder = documents.filter((d) => d.folderId === folder.id);
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
    data-reorder-kind="doc"
    data-reorder-id={doc.id}
    data-reorder-section={section}
    onpointerdown={(e) => handlePointerDown(e, "doc", doc.id, section)}
    onclick={() => {
      if (suppressClick) {
        suppressClick = false;
        return;
      }
      onSelect(doc.id);
    }}
    onkeydown={(e) => activateOn(e, () => onSelect(doc.id))}
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

{#snippet sourceItem(doc: Document)}
  <div
    class="sidebar-item {doc.id === selectedDocId ? 'sidebar-item--active' : ''}"
    class:sidebar-item--dragging={dragSource?.id === doc.id}
    class:sidebar-item--drop-before={dropTarget?.id === doc.id && dropTarget.edge === "before"}
    class:sidebar-item--drop-after={dropTarget?.id === doc.id && dropTarget.edge === "after"}
    data-reorder-kind="doc"
    data-reorder-id={doc.id}
    data-reorder-section="sources"
    onpointerdown={(e) => handlePointerDown(e, "doc", doc.id, "sources")}
    onclick={() => {
      if (suppressClick) {
        suppressClick = false;
        return;
      }
      onOpenSource(doc.id);
    }}
    onkeydown={(e) => activateOn(e, () => onOpenSource(doc.id))}
    role="button"
    tabindex="0"
  >
    <div class="sidebar-item-icon">
      <DocumentIcon type={doc.type} size={16} />
    </div>
    <div class="sidebar-item-info">
      <span class="sidebar-item-name">{doc.name}</span>
      <span class="sidebar-item-date">{formatDate(doc.updatedAt)}</span>
    </div>
    <div class="sidebar-item-actions">
      <button
        class="sidebar-action-btn sidebar-action-btn--delete"
        onclick={(e) => {
          e.stopPropagation();
          onRemoveSource(doc.id);
        }}
        title="Remove source"
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M3 6h18 M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6 M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
        </svg>
      </button>
    </div>
  </div>
{/snippet}

{#snippet ideaItem(doc: Document)}
  <div
    class="sidebar-item {doc.id === selectedDocId ? 'sidebar-item--active' : ''}"
    class:sidebar-item--dragging={dragSource?.id === doc.id}
    class:sidebar-item--drop-before={dropTarget?.id === doc.id && dropTarget.edge === "before"}
    class:sidebar-item--drop-after={dropTarget?.id === doc.id && dropTarget.edge === "after"}
    data-reorder-kind="doc"
    data-reorder-id={doc.id}
    data-reorder-section="inbox"
    onpointerdown={(e) => handlePointerDown(e, "doc", doc.id, "inbox")}
    onclick={() => {
      if (suppressClick) {
        suppressClick = false;
        return;
      }
      onOpenIdea(doc.id);
    }}
    onkeydown={(e) => activateOn(e, () => onOpenIdea(doc.id))}
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

<aside class="sidebar" class:sidebar--reordering={dragSource !== null}>
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
        <button
          class="sidebar-search-result"
          onclick={() =>
            hit.type === "idea"
              ? onOpenIdea(hit.id)
              : hit.type === "source"
                ? onOpenSource(hit.id)
                : onSelect(hit.id)}
        >
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
        <div class="sidebar-search-empty">{searchFailed ? "Search failed — try again." : "No matches"}</div>
      {/if}
    </nav>
  {:else}
  <div class="sidebar-section-header">
    <span class="sidebar-section-label">Ideas</span>
    <div class="sidebar-section-actions">
      {#if tree.ideas.length > 0}
        <button class="sidebar-clear-btn" onclick={onClearIdeas} title="Clear inbox">
          Clear
        </button>
      {/if}
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
      <button class="sidebar-new-btn" onclick={onImport} title="Import files (Markdown, text, PDF, Word)">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
          <polyline points="7 10 12 15 17 10" />
          <line x1="12" y1="15" x2="12" y2="3" />
        </svg>
      </button>
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
          class:sidebar-folder-header--drop-into={folderDropId === folder.id}
          data-reorder-kind="folder"
          data-reorder-id={folder.id}
          data-reorder-section="folders"
          data-drop-folder={folder.id}
          onpointerdown={(e) => handlePointerDown(e, "folder", folder.id, "folders")}
          onclick={() => {
            if (suppressClick) {
              suppressClick = false;
              return;
            }
            toggleFolder(folder.id);
          }}
          onkeydown={(e) => activateOn(e, () => toggleFolder(folder.id))}
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

  {#if tree.sources.length > 0}
    <div class="sidebar-section-header">
      <span class="sidebar-section-label">Sources</span>
    </div>
    <nav class="sidebar-inbox" aria-label="Sources">
      {#each tree.sources as src (src.id)}
        {@render sourceItem(src)}
      {/each}
    </nav>
  {/if}
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
