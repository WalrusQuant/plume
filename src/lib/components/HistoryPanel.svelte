<script lang="ts">
  import type { SnapshotMeta, SnapshotCause } from "$lib/api";

  interface Props {
    snapshots: SnapshotMeta[];
    onSaveSnapshot: () => void;
    onRestore: (id: string) => void;
    getSnapshotContent: (id: string) => Promise<string>;
  }

  let { snapshots, onSaveSnapshot, onRestore, getSnapshotContent }: Props = $props();

  // id of the snapshot whose content is expanded inline, plus its loaded text
  let expandedId = $state<string | null>(null);
  let expandedText = $state("");
  // id whose content is currently loading (drives the View button's "…" state)
  let loadingId = $state<string | null>(null);

  const CAUSE_LABELS: Record<SnapshotCause, string> = {
    "ai-edit": "AI edit",
    interval: "Auto",
    manual: "Saved",
    restore: "Pre-restore",
  };

  async function toggleView(id: string) {
    if (expandedId === id) {
      expandedId = null;
      return;
    }
    loadingId = id;
    try {
      expandedText = await getSnapshotContent(id);
      expandedId = id;
    } finally {
      loadingId = null;
    }
  }

  function confirmRestore(id: string) {
    if (confirm("Restore this version? Your current text is saved to history first.")) {
      onRestore(id);
    }
  }

  function relativeTime(iso: string): string {
    const then = new Date(iso).getTime();
    const secs = Math.round((Date.now() - then) / 1000);
    if (secs < 45) return "just now";
    const mins = Math.round(secs / 60);
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.round(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.round(hours / 24);
    if (days < 7) return `${days}d ago`;
    return new Date(iso).toLocaleDateString();
  }
</script>

<div class="history-panel">
  <div class="history-header">
    <button class="dialog-btn dialog-btn--secondary" onclick={onSaveSnapshot}>
      Save snapshot
    </button>
  </div>

  {#if snapshots.length === 0}
    <div class="assistant-empty">
      <p>No snapshots yet. Versions are captured automatically as you write, and you can save one anytime.</p>
    </div>
  {:else}
    <ul class="history-list">
      {#each snapshots as snap (snap.id)}
        <li class="history-item">
          <div class="history-item-row">
            <span class="history-badge history-badge--{snap.cause}">{CAUSE_LABELS[snap.cause]}</span>
            <span class="history-time">{relativeTime(snap.createdAt)}</span>
            <span class="history-words">{snap.wordCount} words</span>
            <div class="history-actions">
              <button class="history-action" onclick={() => toggleView(snap.id)} disabled={loadingId === snap.id}>
                {loadingId === snap.id ? "…" : expandedId === snap.id ? "Hide" : "View"}
              </button>
              <button class="history-action" onclick={() => confirmRestore(snap.id)}>Restore</button>
            </div>
          </div>
          {#if expandedId === snap.id}
            <pre class="history-preview">{expandedText}</pre>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>
