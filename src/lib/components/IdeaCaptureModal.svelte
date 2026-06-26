<script lang="ts">
  import { toast } from "$lib/toast.svelte";
  import { formatError } from "$lib/formatError";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import Dialog from "$lib/components/Dialog.svelte";

  // Small capture/edit modal for ideas. Ideas are quick notes — they never open
  // in the big editor, so all capture and editing happens here. Reuses the
  // shared .dialog-* shell. Only rendered while `open` is true, so local title/
  // body state always reseeds from props on open (no stale leakage).
  interface Props {
    open: boolean;
    mode: "new" | "edit";
    initialTitle: string;
    initialBody: string;
    /** title === "" means "derive the name from the first line". Resolves once
        the save persists, so the modal can stay open if it fails. */
    onSave: (title: string, body: string) => Promise<void>;
    onClose: () => void;
  }

  let { open, mode, initialTitle, initialBody, onSave, onClose }: Props = $props();

  let title = $state("");
  let body = $state("");
  let saving = $state(false);

  const dirty = $derived(title !== initialTitle || body !== initialBody);

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

  async function handleSave() {
    if (saving) return;
    saving = true;
    try {
      await onSave(title.trim(), body); // await so a failed save keeps the text
    } catch (e) {
      toast.error(`Save idea failed: ${formatError(e)}`);
      return; // leave the modal open with the user's text intact
    } finally {
      saving = false;
    }
    onClose();
  }

  /** Dismiss, guarding unsaved edits so a stray click doesn't drop a captured idea. */
  async function requestClose() {
    if (dirty && !(await confirm("Discard this idea?"))) return;
    onClose();
  }

  function handleKeyDown(e: KeyboardEvent) {
    // Cmd/Ctrl+Enter saves; plain Enter must insert newlines in the textarea.
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void handleSave();
    }
  }
</script>

<Dialog
  {open}
  title={mode === "new" ? "New Idea" : "Edit Idea"}
  onClose={requestClose}
  onOverlayClick={requestClose}
>
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
      onkeydown={handleKeyDown}
      use:focusOnMount
    ></textarea>
  </div>

  {#snippet footer()}
    <div class="dialog-footer">
      <span class="idea-save-hint">⌘↵ to save</span>
      <button class="dialog-btn dialog-btn--secondary" onclick={requestClose}>Cancel</button>
      <button class="dialog-btn dialog-btn--primary" onclick={handleSave} disabled={saving}>
        {mode === "new" ? "Save" : "Save changes"}
      </button>
    </div>
  {/snippet}
</Dialog>

<style>
  .idea-save-hint {
    margin-right: auto;
    align-self: center;
    font-size: 11.5px;
    color: var(--text-tertiary);
  }
</style>
