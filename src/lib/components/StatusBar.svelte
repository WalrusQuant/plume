<script lang="ts">
  interface Props {
    documentName: string;
    cursorPosition: { line: number; col: number };
    wordCount: number;
    saveStatus?: "saved" | "saving" | "unsaved" | "error";
  }

  let { documentName, cursorPosition, wordCount, saveStatus = "saved" }: Props = $props();

  const label = $derived(
    saveStatus === "saving"
      ? "Saving…"
      : saveStatus === "unsaved"
        ? "Unsaved changes"
        : saveStatus === "error"
          ? "Save failed — will retry"
          : "Saved",
  );
</script>

<div class="status-bar">
  <div class="status-bar-left">
    <span class="status-save status-save--{saveStatus}" role="status" aria-live="polite">
      {label}
    </span>
    <span class="status-text">{wordCount} {wordCount === 1 ? "word" : "words"}</span>
  </div>
  <div class="status-bar-center">
    <span class="status-doc-name">{documentName}</span>
  </div>
  <div class="status-bar-right">
    <span class="status-cursor">Ln {cursorPosition.line}, Col {cursorPosition.col}</span>
  </div>
</div>
