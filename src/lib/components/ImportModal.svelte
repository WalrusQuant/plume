<script lang="ts">
  import Dialog from "$lib/components/Dialog.svelte";

  interface Props {
    open: boolean;
    /** Absolute paths of the files chosen/dropped, awaiting a mode choice. */
    files: string[];
    /** Called with the chosen mode: true = read-only searchable sources,
        false = editable documents. */
    onChoose: (asSource: boolean) => void;
    onClose: () => void;
  }

  let { open, files, onChoose, onClose }: Props = $props();

  const count = $derived(files.length);
  const names = $derived(
    files.map((p) => p.split(/[/\\]/).pop() ?? p),
  );
</script>

<Dialog {open} title="Import files" {onClose}>
  <div class="dialog-body">
    <p class="import-count">
      {count}
      {count === 1 ? "file" : "files"} to import:
    </p>
    <ul class="import-file-list">
      {#each names as name (name)}
        <li class="import-file">{name}</li>
      {/each}
    </ul>

    <p class="dialog-label">How should these be added?</p>
    <div class="import-choices">
      <button type="button" class="import-choice" onclick={() => onChoose(false)}>
        <span class="import-choice-title">As documents</span>
        <span class="import-choice-desc">
          Editable pages in your document list. Open, edit, and chat about them.
        </span>
      </button>
      <button type="button" class="import-choice" onclick={() => onChoose(true)}>
        <span class="import-choice-title">As sources</span>
        <span class="import-choice-desc">
          Read-only reference material in the Sources section — searchable by the
          assistant, but not edited.
        </span>
      </button>
    </div>
  </div>

  {#snippet footer()}
    <div class="dialog-footer">
      <button type="button" class="dialog-btn dialog-btn--secondary" onclick={onClose}>
        Cancel
      </button>
    </div>
  {/snippet}
</Dialog>

<style>
  .import-count {
    margin: 0 0 6px;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .import-file-list {
    margin: 0 0 16px;
    padding: 0;
    list-style: none;
    max-height: 8rem;
    overflow-y: auto;
  }
  .import-file {
    padding: 3px 0;
    font-size: 12.5px;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .import-choices {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 8px;
  }
  .import-choice {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 12px 14px;
    text-align: left;
    border: 1px solid var(--border);
    border-radius: var(--radius-md, 8px);
    background: var(--toolbar-bg);
    color: var(--text-primary);
    cursor: pointer;
    transition: border-color 0.15s ease, background 0.15s ease;
  }
  .import-choice:hover {
    border-color: var(--accent);
    background: var(--accent-surface);
  }
  .import-choice-title {
    font-size: 13.5px;
    font-weight: 600;
  }
  .import-choice-desc {
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.4;
  }
</style>
