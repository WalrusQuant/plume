<script lang="ts">
  import type { DocType } from "$lib/api";
  import { DOCUMENT_TYPES } from "$lib/documentTypes";
  import DocumentIcon from "$lib/components/DocumentIcon.svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
    onCreate: (name: string, type: DocType) => void;
  }

  let { open, onClose, onCreate }: Props = $props();

  let selectedType = $state<DocType>("generic");
  let name = $state("");

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
    if (e.key === "Escape") onClose();
  }
</script>

{#if open}
  <div class="dialog-overlay" onclick={onClose} role="presentation">
    <div
      class="dialog"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleKeyDown}
      role="dialog"
      tabindex="-1"
    >
      <div class="dialog-header">
        <h3 class="dialog-title">New Document</h3>
        <button class="dialog-close" onclick={onClose} title="Close">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>

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
          use:focusOnMount
        />
      </div>

      <div class="dialog-footer">
        <button class="dialog-btn dialog-btn--secondary" onclick={onClose}>Cancel</button>
        <button class="dialog-btn dialog-btn--primary" onclick={handleCreate}>Create</button>
      </div>
    </div>
  </div>
{/if}
