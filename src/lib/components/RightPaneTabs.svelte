<script lang="ts">
  export type RightPaneTab = "preview" | "assistant";

  interface Props {
    activeTab: RightPaneTab;
    onTabChange: (tab: RightPaneTab) => void;
  }

  let { activeTab, onTabChange }: Props = $props();
</script>

<div class="right-pane-tabs">
  <button
    class="right-pane-tab {activeTab === 'preview' ? 'right-pane-tab--active' : ''}"
    onclick={() => onTabChange("preview")}
  >
    Preview
  </button>
  <button
    class="right-pane-tab {activeTab === 'assistant' ? 'right-pane-tab--active' : ''}"
    onclick={() => onTabChange("assistant")}
  >
    Assistant
  </button>
</div>
