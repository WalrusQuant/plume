<script lang="ts">
  import type { DocType } from "$lib/api";
  import { DOCUMENT_TYPES } from "$lib/documentTypes";
  import DocumentIcon from "$lib/components/DocumentIcon.svelte";
  import Dialog from "$lib/components/Dialog.svelte";

  interface Props {
    open: boolean;
    /** Pre-selected type when the dialog opens (e.g. "plan" from the shelf's + New menu). */
    initialType?: DocType;
    onClose: () => void;
    onCreate: (name: string, type: DocType) => void;
  }

  let { open, initialType = "generic", onClose, onCreate }: Props = $props();

  let selectedType = $state<DocType>("generic");
  let name = $state("");

  $effect(() => {
    if (open) {
      selectedType = initialType;
      name = "";
    }
  });

  const selectedConfig = $derived(DOCUMENT_TYPES.find((t) => t.type === selectedType));

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
  }

  function handleCreate() {
    const fallback = selectedType === "generic" ? "Untitled" : selectedConfig?.label;
    const docName = name.trim() || fallback || "Untitled";
    onCreate(docName, selectedType);
    name = "";
    selectedType = "generic";
    onClose();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleCreate();
    }
  }
</script>

<Dialog {open} title="New Document" {onClose}>
  <div class="dialog-body">
    <label class="dialog-label" for="doc-type-grid">Document type</label>
    <div class="dialog-type-grid" id="doc-type-grid">
      {#each DOCUMENT_TYPES as dt (dt.type)}
        <button
          class="dialog-type-card {selectedType === dt.type ? 'dialog-type-card--active' : ''}"
          onclick={() => (selectedType = dt.type)}
        >
          <div class="dialog-type-icon">
            <DocumentIcon type={dt.type} size={20} />
          </div>
          <div class="dialog-type-info">
            <span class="dialog-type-label">{dt.label}</span>
            <span class="dialog-type-desc">{dt.description}</span>
          </div>
        </button>
      {/each}
    </div>

    <label class="dialog-label" for="doc-name">Name</label>
    <input
      id="doc-name"
      class="dialog-input"
      type="text"
      bind:value={name}
      placeholder={selectedConfig?.label}
      onkeydown={handleKeyDown}
      use:focusOnMount
    />
  </div>

  {#snippet footer()}
    <div class="dialog-footer">
      <button class="dialog-btn dialog-btn--secondary" onclick={onClose}>Cancel</button>
      <button class="dialog-btn dialog-btn--primary" onclick={handleCreate}>Create</button>
    </div>
  {/snippet}
</Dialog>
