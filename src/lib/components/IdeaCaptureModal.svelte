<script lang="ts">
  // Small capture/edit modal for ideas. Ideas are quick notes — they never open
  // in the big editor, so all capture and editing happens here. Reuses the
  // shared .dialog-* shell. Only rendered while `open` is true, so local title/
  // body state always reseeds from props on open (no stale leakage).
  interface Props {
    open: boolean;
    mode: "new" | "edit";
    initialTitle: string;
    initialBody: string;
    /** title === "" means "derive the name from the first line". */
    onSave: (title: string, body: string) => void;
    onClose: () => void;
  }

  let { open, mode, initialTitle, initialBody, onSave, onClose }: Props = $props();

  let title = $state("");
  let body = $state("");

  // Reseed the fields each time the modal opens (it stays mounted between
  // opens, so we can't rely on a fresh mount). While open, local edits aren't
  // deps here, so typing never re-triggers this.
  $effect(() => {
    if (open) {
      title = initialTitle;
      body = initialBody;
    }
  });

  function focusOnMount(node: HTMLTextAreaElement) {
    node.focus();
    // put the cursor at the end so editing an existing idea is comfortable
    const len = node.value.length;
    node.setSelectionRange(len, len);
  }

  function handleSave() {
    onSave(title.trim(), body);
    onClose();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
    // Cmd/Ctrl+Enter saves; plain Enter must insert newlines in the textarea.
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSave();
    }
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
        <h3 class="dialog-title">{mode === "new" ? "New Idea" : "Edit Idea"}</h3>
        <button class="dialog-close" onclick={onClose} title="Close">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>

      <div class="dialog-body">
        <input
          class="dialog-input"
          type="text"
          bind:value={title}
          placeholder="Title (optional)"
        />
        <textarea
          class="dialog-textarea"
          bind:value={body}
          placeholder="Capture a quick idea…"
          use:focusOnMount
        ></textarea>
      </div>

      <div class="dialog-footer">
        <button class="dialog-btn dialog-btn--secondary" onclick={onClose}>Cancel</button>
        <button class="dialog-btn dialog-btn--primary" onclick={handleSave}>
          {mode === "new" ? "Save" : "Save changes"}
        </button>
      </div>
    </div>
  </div>
{/if}
