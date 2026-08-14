<script lang="ts">
  export type RightPaneTab = "preview" | "assistant" | "history" | "cheatsheet";

  interface Props {
    activeTab: RightPaneTab;
    onTabChange: (tab: RightPaneTab) => void;
  }

  let { activeTab, onTabChange }: Props = $props();

  const TABS: { id: RightPaneTab; label: string }[] = [
    { id: "assistant", label: "Assistant" },
    { id: "preview", label: "Preview" },
    { id: "history", label: "History" },
    { id: "cheatsheet", label: "Guide" },
  ];

  function onKeydown(e: KeyboardEvent) {
    const i = TABS.findIndex((t) => t.id === activeTab);
    if (e.key === "ArrowRight") {
      e.preventDefault();
      onTabChange(TABS[(i + 1) % TABS.length].id);
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      onTabChange(TABS[(i - 1 + TABS.length) % TABS.length].id);
    } else if (e.key === "Home") {
      e.preventDefault();
      onTabChange(TABS[0].id);
    } else if (e.key === "End") {
      e.preventDefault();
      onTabChange(TABS[TABS.length - 1].id);
    }
  }
</script>

<!-- svelte-ignore a11y_interactive_supports_focus -->
<div class="right-pane-tabs" role="tablist" aria-label="Right pane" onkeydown={onKeydown}>
  {#each TABS as tab (tab.id)}
    <button
      class="right-pane-tab {activeTab === tab.id ? 'right-pane-tab--active' : ''}"
      role="tab"
      id={`right-tab-${tab.id}`}
      aria-selected={activeTab === tab.id}
      aria-controls={`right-panel-${tab.id}`}
      tabindex={activeTab === tab.id ? 0 : -1}
      title={tab.label}
      onclick={() => onTabChange(tab.id)}
    >
      <svg class="right-pane-tab-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        {#if tab.id === "assistant"}
          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
        {:else if tab.id === "preview"}
          <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
          <circle cx="12" cy="12" r="3" />
        {:else if tab.id === "history"}
          <circle cx="12" cy="12" r="10" />
          <polyline points="12 6 12 12 16 14" />
        {:else}
          <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
          <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
        {/if}
      </svg>
      <span class="right-pane-tab-label">{tab.label}</span>
    </button>
  {/each}
</div>
