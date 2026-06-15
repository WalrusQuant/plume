<script lang="ts">
  import { api, type Document, type Folder, type SearchHit } from "$lib/api";
  import type { Theme } from "$lib/editor/themes";
  import { buildSidebarTree, type SidebarFolder } from "$lib/buildSidebarTree";
  import { formatDate } from "$lib/formatDate";
  import { clickOutside } from "$lib/clickOutside";
  import DocumentIcon from "$lib/components/DocumentIcon.svelte";

  interface Props {
    documents: Document[];
    folders: Folder[];
    onOpenDocument: (id: string) => void;
    onOpenIdea: (id: string) => void;
    onCreateProject: (name: string) => Promise<Folder>;
    /** Open the new-document dialog; the created doc lands in `folderId` (null = unfiled). */
    onNewPage: (folderId: string | null) => void;
    onNewPlan: () => void;
    onNewIdea: () => void;
    onToggleActive: (id: string, active: boolean) => void;
    /** Whether an AI provider key is set — drives the first-run setup nudge. */
    isConfigured: boolean;
    // TopBar doesn't render on home, so the shelf carries its own quiet controls.
    theme: Theme;
    onToggleTheme: () => void;
    onOpenSettings: () => void;
  }

  let {
    documents,
    folders,
    onOpenDocument,
    onOpenIdea,
    onCreateProject,
    onNewPage,
    onNewPlan,
    onNewIdea,
    onToggleActive,
    isConfigured,
    theme,
    onToggleTheme,
    onOpenSettings,
  }: Props = $props();

  const tree = $derived(buildSidebarTree(folders, documents));

  /** A project's freshness is its most-recently-touched doc. Its `documents`
      are now in manual order (not recency), so scan for the max; fall back to
      the folder's own timestamp when it has no docs. */
  function freshness(f: SidebarFolder): number {
    const newest = f.documents.reduce(
      (max, d) => Math.max(max, new Date(d.updatedAt).getTime()),
      0,
    );
    return newest || new Date(f.updatedAt).getTime();
  }
  const activeProjects = $derived(
    [...tree.folderTree.filter((f) => f.active)].sort((a, b) => freshness(b) - freshness(a)),
  );
  const restingProjects = $derived(tree.folderTree.filter((f) => !f.active));
  const recent = $derived(documents.filter((d) => d.type !== "idea").slice(0, 8));
  const inbox = $derived(tree.ideas.slice(0, 8));
  const isEmpty = $derived(folders.length === 0 && recent.length === 0);

  // Cross-document search, same debounced + seq-guarded pattern as the
  // sidebar's (which isn't visible on home). A non-empty query swaps the
  // shelf body for ranked results.
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

  let newMenuOpen = $state(false);
  let expanded = $state(new Set<string>());

  function toggleExpanded(id: string) {
    const next = new Set(expanded);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    expanded = next;
  }

  // Inline new-project row: calm, no dialog. The row is created on Enter.
  let creatingProject = $state(false);
  let newProjectName = $state("");

  function startNewProject() {
    newMenuOpen = false;
    creatingProject = true;
    newProjectName = "";
  }

  async function commitNewProject() {
    const name = newProjectName.trim();
    creatingProject = false;
    newProjectName = "";
    if (name) await onCreateProject(name);
  }

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
  }
</script>

{#snippet listRow(doc: Document, open: (id: string) => void)}
  <button class="shelf-list-row" onclick={() => open(doc.id)}>
    <span class="shelf-list-icon"><DocumentIcon type={doc.type} size={14} /></span>
    <span class="shelf-list-name">{doc.name}</span>
    <span class="shelf-list-date">{formatDate(doc.updatedAt)}</span>
  </button>
{/snippet}

{#snippet projectRow(project: SidebarFolder, resting: boolean)}
  <div class="shelf-project {resting ? 'shelf-project--resting' : ''}">
    <div
      class="shelf-project-header"
      onclick={() => toggleExpanded(project.id)}
      onkeydown={(e) => e.key === "Enter" && toggleExpanded(project.id)}
      role="button"
      tabindex="0"
    >
      <svg
        class="shelf-project-chevron {expanded.has(project.id) ? 'shelf-project-chevron--open' : ''}"
        width="12"
        height="12"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <polyline points="9 18 15 12 9 6" />
      </svg>
      <span class="shelf-project-name">{project.name}</span>
      <span class="shelf-project-count">
        {project.documents.length}
        {project.documents.length === 1 ? "doc" : "docs"}
      </span>
      <button
        class="shelf-rest-btn"
        onclick={(e) => {
          e.stopPropagation();
          onToggleActive(project.id, resting);
        }}
        title={resting ? "Move back to the top of the shelf" : "Set aside — the project keeps everything"}
      >
        {resting ? "Make active" : "Rest"}
      </button>
    </div>
    {#if expanded.has(project.id)}
      <div class="shelf-project-docs">
        {#each project.documents as doc (doc.id)}
          {@render listRow(doc, onOpenDocument)}
        {/each}
        {#if project.documents.length === 0}
          <span class="shelf-quiet">No pages yet</span>
        {/if}
        <button class="shelf-add-page" onclick={() => onNewPage(project.id)}>+ New page</button>
      </div>
    {:else if !resting}
      <div class="shelf-project-hint">
        {#each project.documents.slice(0, 3) as doc, i (doc.id)}
          {#if i > 0}<span class="shelf-hint-sep">·</span>{/if}
          <button class="shelf-hint-chip" onclick={() => onOpenDocument(doc.id)}>{doc.name}</button>
        {/each}
        {#if project.documents.length > 3}
          <span class="shelf-hint-sep">· …</span>
        {/if}
        {#if project.documents.length === 0}
          <span class="shelf-quiet">No pages yet</span>
        {/if}
      </div>
    {/if}
  </div>
{/snippet}

<div class="shelf">
  <div class="shelf-inner">
    <header class="shelf-header">
      <h1 class="shelf-title">Your notebook</h1>
      <div class="shelf-controls">
        <div class="shelf-new" use:clickOutside={() => (newMenuOpen = false)}>
          <button class="shelf-new-btn" onclick={() => (newMenuOpen = !newMenuOpen)}>
            + New
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="6 9 12 15 18 9" />
            </svg>
          </button>
          {#if newMenuOpen}
            <div class="shelf-menu" role="menu" tabindex="-1">
              <button class="shelf-menu-item" onclick={startNewProject}>New project</button>
              <button
                class="shelf-menu-item"
                onclick={() => {
                  newMenuOpen = false;
                  onNewPage(null);
                }}
              >
                New doc
              </button>
              <button
                class="shelf-menu-item"
                onclick={() => {
                  newMenuOpen = false;
                  onNewIdea();
                }}
              >
                New idea
              </button>
              <button
                class="shelf-menu-item"
                onclick={() => {
                  newMenuOpen = false;
                  onNewPlan();
                }}
              >
                New plan
              </button>
            </div>
          {/if}
        </div>
        <button class="shelf-icon-btn" onclick={onOpenSettings} title="Settings">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3" />
            <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
          </svg>
        </button>
        <button
          class="shelf-icon-btn"
          onclick={onToggleTheme}
          title={`Switch to ${theme === "dark" ? "light" : "dark"} mode`}
        >
          {#if theme === "dark"}
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="5" />
              <line x1="12" y1="1" x2="12" y2="3" />
              <line x1="12" y1="21" x2="12" y2="23" />
              <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
              <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
              <line x1="1" y1="12" x2="3" y2="12" />
              <line x1="21" y1="12" x2="23" y2="12" />
              <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
              <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
            </svg>
          {:else}
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
            </svg>
          {/if}
        </button>
      </div>
    </header>

    {#if !isEmpty}
      <div class="shelf-search">
        <svg class="shelf-search-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="11" cy="11" r="7" />
          <line x1="21" y1="21" x2="16.65" y2="16.65" />
        </svg>
        <input
          class="shelf-search-input"
          type="text"
          placeholder="Search your notebook…"
          bind:value={searchQuery}
        />
        {#if searching}
          <button class="shelf-search-clear" onclick={() => (searchQuery = "")} title="Clear search">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        {/if}
      </div>
    {/if}

    {#if searching}
      <section class="shelf-search-results">
        {#each searchResults as hit (hit.id)}
          <button class="shelf-list-row" onclick={() => onOpenDocument(hit.id)}>
            <span class="shelf-list-icon"><DocumentIcon type={hit.type} size={14} /></span>
            <span class="shelf-search-result-body">
              <span class="shelf-list-name">{hit.name}</span>
              {#if hit.snippet}
                <span class="shelf-search-snippet">{hit.snippet}</span>
              {/if}
            </span>
          </button>
        {/each}
        {#if searchResults.length === 0}
          <span class="shelf-quiet">No matches</span>
        {/if}
      </section>
    {:else if isEmpty && !creatingProject}
      <div class="shelf-empty">
        <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" opacity="0.2">
          <polyline points="4 17 10 11 4 5" />
          <line x1="12" y1="19" x2="20" y2="19" />
        </svg>
        <h2>Welcome to Plume</h2>
        <p>Write in markdown, then let AI reshape a finished piece into a version
          for every platform. A <strong>project</strong> keeps a piece and its
          versions together; the <strong>Inbox</strong> holds quick ideas you can
          expand into drafts later.</p>
        <div class="shelf-empty-actions">
          <button class="shelf-empty-btn" onclick={startNewProject} title="Group a piece and its platform versions">Start a project</button>
          <button class="shelf-empty-btn" onclick={onNewPlan} title="A structured plan document">Write a plan</button>
          <button class="shelf-empty-btn" onclick={() => onNewPage(null)} title="A blank markdown document">New document</button>
          <button class="shelf-empty-btn" onclick={onNewIdea} title="A quick note for the Inbox">Capture an idea</button>
        </div>
        {#if !isConfigured}
          <button class="shelf-empty-ai" onclick={onOpenSettings}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M5 3v4M3 5h4M6 17v4M4 19h4M13 3l2.5 6.5L22 12l-6.5 2.5L13 21l-2.5-6.5L4 12l6.5-2.5L13 3z" />
            </svg>
            Set up an AI provider to unlock writing help
          </button>
        {/if}
      </div>
    {:else}
      <section class="shelf-projects">
        {#if creatingProject}
          <div class="shelf-new-project">
            <input
              class="shelf-new-project-input"
              type="text"
              placeholder="Project name…"
              bind:value={newProjectName}
              onblur={() => void commitNewProject()}
              onkeydown={(e) => {
                if (e.key === "Enter") void commitNewProject();
                if (e.key === "Escape") {
                  newProjectName = ""; // clear first so the blur commit is a no-op
                  creatingProject = false;
                }
              }}
              use:focusOnMount
            />
          </div>
        {/if}
        {#each activeProjects as project (project.id)}
          {@render projectRow(project, false)}
        {/each}
        {#if restingProjects.length > 0}
          <div class="shelf-resting-label">Resting</div>
          {#each restingProjects as project (project.id)}
            {@render projectRow(project, true)}
          {/each}
        {/if}
      </section>

      <div class="shelf-columns">
        <section class="shelf-col">
          <h2 class="shelf-section-title">Inbox</h2>
          {#each inbox as idea (idea.id)}
            {@render listRow(idea, onOpenIdea)}
          {/each}
          {#if inbox.length === 0}
            <button class="shelf-list-empty" onclick={onNewIdea}>Capture a quick idea…</button>
          {/if}
        </section>
        <section class="shelf-col">
          <h2 class="shelf-section-title">Recent</h2>
          {#each recent as doc (doc.id)}
            {@render listRow(doc, onOpenDocument)}
          {/each}
          {#if recent.length === 0}
            <span class="shelf-quiet">Nothing yet</span>
          {/if}
        </section>
      </div>
    {/if}
  </div>
</div>
