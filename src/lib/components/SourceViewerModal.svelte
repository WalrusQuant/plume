<script lang="ts">
  import { api, type Document } from "$lib/api";
  import { toast } from "$lib/toast.svelte";
  import { formatError } from "$lib/formatError";
  import Dialog from "$lib/components/Dialog.svelte";

  interface Props {
    open: boolean;
    source: Document | null;
    onClose: () => void;
    /** Remove the source (delete it — its search index entries cascade away). */
    onRemove: (id: string) => void;
    /** Promote the source to an editable document and open it. */
    onConvert: (id: string, name: string) => void;
  }

  let { open, source, onClose, onRemove, onConvert }: Props = $props();

  let content = $state("");
  let loading = $state(false);

  // Load the source body whenever the viewer opens for a source.
  $effect(() => {
    if (!open || !source) return;
    const id = source.id;
    loading = true;
    content = "";
    void api
      .getDocumentContent(id)
      .then((c) => {
        if (source?.id === id) content = c;
      })
      .catch((e) => toast.error(`Couldn't load source: ${formatError(e)}`))
      .finally(() => (loading = false));
  });
</script>

<Dialog {open} title={source?.name ?? "Source"} {onClose}>
  <div class="dialog-body">
    <p class="source-viewer-note">
      Read-only reference. The assistant can search this when “Search your notes” is on.
    </p>
    {#if loading}
      <div class="source-viewer-loading">Loading…</div>
    {:else}
      <pre class="source-viewer-body">{content}</pre>
    {/if}
  </div>

  {#snippet footer()}
    <div class="dialog-footer">
      <button
        type="button"
        class="dialog-btn dialog-btn--secondary"
        onclick={() => source && onConvert(source.id, source.name)}
      >
        Convert to document
      </button>
      <button
        type="button"
        class="dialog-btn source-remove-btn"
        onclick={() => source && onRemove(source.id)}
      >
        Remove source
      </button>
    </div>
  {/snippet}
</Dialog>

<style>
  .source-viewer-note {
    margin: 0 0 10px;
    font-size: 12px;
    color: var(--text-secondary);
  }
  .source-viewer-body {
    margin: 0;
    max-height: 55vh;
    overflow-y: auto;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm, 4px);
    background: var(--editor-bg);
    color: var(--text-primary);
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 12.5px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
  .source-viewer-loading {
    padding: 24px;
    text-align: center;
    color: var(--text-secondary);
    font-size: 13px;
  }
  .source-remove-btn {
    color: var(--error, #e5484d);
    border-color: var(--border);
  }
</style>
