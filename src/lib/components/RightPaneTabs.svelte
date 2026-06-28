<script lang="ts">
  export type RightPaneTab = "preview" | "assistant" | "history" | "cheatsheet";

  interface Props {
    activeTab: RightPaneTab;
    onTabChange: (tab: RightPaneTab) => void;
  }

  let { activeTab, onTabChange }: Props = $props();

  const TABS: { id: RightPaneTab; label: string }[] = [
    { id: "preview", label: "Preview" },
    { id: "assistant", label: "Assistant" },
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
      onclick={() => onTabChange(tab.id)}
    >
      {tab.label}
    </button>
  {/each}
</div>
