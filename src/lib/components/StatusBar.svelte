<script lang="ts">
  interface Props {
    documentName: string;
    cursorPosition: { line: number; col: number };
    wordCount: number;
  }

  let { documentName, cursorPosition, wordCount }: Props = $props();
</script>

<div class="status-bar">
  <div class="status-bar-left">
    <span class="status-text">{wordCount} {wordCount === 1 ? "word" : "words"}</span>
  </div>
  <div class="status-bar-center">
    <span class="status-doc-name">{documentName}</span>
  </div>
  <div class="status-bar-right">
    <span class="status-cursor">Ln {cursorPosition.line}, Col {cursorPosition.col}</span>
  </div>
</div>
