<script lang="ts">
  import type { Theme } from "$lib/editor/themes";
  import type { ExportTarget } from "$lib/api";
  import { clickOutside } from "$lib/clickOutside";

  interface Props {
    documentName: string;
    theme: Theme;
    onToggleTheme: () => void;
    sidebarCollapsed: boolean;
    onToggleSidebar: () => void;
    onRename: (name: string) => void;
    exportTargets: ExportTarget[];
    onExport: (targetId: string) => void;
    exportStatus: string;
    onMultiply: () => void;
    onOpenSettings: () => void;
  }

  let {
    documentName,
    theme,
    onToggleTheme,
    sidebarCollapsed,
    onToggleSidebar,
    onRename,
    exportTargets,
    onExport,
    exportStatus,
    onMultiply,
    onOpenSettings,
  }: Props = $props();

  let exportOpen = $state(false);

  let editing = $state(false);
  let editName = $state("");

  function startEdit() {
    editName = documentName;
    editing = true;
  }

  function commitEdit() {
    if (editing && editName.trim() && editName.trim() !== documentName) {
      onRename(editName.trim());
    }
    editing = false;
  }

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

<div class="topbar">
  <div class="topbar-left">
    <button
      class="topbar-sidebar-btn"
      onclick={onToggleSidebar}
      title={sidebarCollapsed ? "Show sidebar" : "Hide sidebar"}
    >
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <rect x="3" y="3" width="18" height="18" rx="2" />
        <line x1="9" y1="3" x2="9" y2="21" />
        {#if sidebarCollapsed}
          <path d="M14 9l3 3-3 3" />
        {:else}
          <path d="M15 9l-3 3 3 3" />
        {/if}
      </svg>
    </button>
    {#if editing}
      <input
        class="topbar-rename-input"
        bind:value={editName}
        onblur={commitEdit}
        onkeydown={(e) => {
          if (e.key === "Enter") commitEdit();
          if (e.key === "Escape") editing = false;
        }}
        use:focusOnMount
      />
    {:else}
      <button class="topbar-doc-name topbar-doc-name--editable" onclick={startEdit} title="Rename document">
        {documentName || "Untitled"}
      </button>
    {/if}
  </div>
  <div class="topbar-right">
    {#if exportStatus}
      <span class="topbar-export-status">{exportStatus}</span>
    {/if}
    <button class="topbar-theme-btn" onclick={onMultiply} title="Multiply into platform versions">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="6" cy="6" r="3" />
        <circle cx="18" cy="6" r="3" />
        <circle cx="18" cy="18" r="3" />
        <path d="M9 6h6M18 9v6M15.5 7.5l-7 9" />
      </svg>
    </button>
    <div class="topbar-export" use:clickOutside={() => (exportOpen = false)}>
      <button class="topbar-theme-btn" onclick={() => (exportOpen = !exportOpen)} title="Export">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
          <polyline points="7 10 12 15 17 10" />
          <line x1="12" y1="15" x2="12" y2="3" />
        </svg>
      </button>
      {#if exportOpen}
        <div class="export-menu" role="menu" tabindex="-1">
          {#each exportTargets as target (target.id)}
            <button
              class="export-menu-item"
              onclick={() => {
                exportOpen = false;
                onExport(target.id);
              }}
            >
              {target.label}
              <span class="export-menu-hint">
                {target.delivery === "clipboard" ? "copies" : `.${target.ext}`}
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <button class="topbar-theme-btn" onclick={onOpenSettings} title="Settings">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="3" />
        <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
      </svg>
    </button>
    <button
      class="topbar-theme-btn"
      onclick={onToggleTheme}
      title={`Switch to ${theme === "dark" ? "light" : "dark"} mode`}
    >
      {#if theme === "dark"}
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
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
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
        </svg>
      {/if}
    </button>
  </div>
</div>
