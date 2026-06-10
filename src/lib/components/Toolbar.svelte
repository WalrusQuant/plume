<script lang="ts">
  import { undo, redo } from "@codemirror/commands";
  import type { EditorView } from "@codemirror/view";
  import {
    toggleBold,
    toggleItalic,
    toggleInlineCode,
    toggleStrikethrough,
    insertHeading,
    insertBlockquote,
    insertBulletList,
    insertNumberedList,
    insertTaskList,
    insertLink,
    insertImage,
    insertCodeBlock,
    insertTable,
    insertHorizontalRule,
  } from "$lib/editor/formatting";

  interface Props {
    editorView: EditorView | null;
  }

  let { editorView }: Props = $props();

  function run(fn: (view: EditorView) => void) {
    if (editorView) fn(editorView);
  }
</script>

{#snippet icon(paths: string)}
  <svg
    width="16"
    height="16"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
  >
    <!-- eslint-disable-next-line svelte/no-at-html-tags — static icon path data only -->
    {@html paths}
  </svg>
{/snippet}

<div class="toolbar">
  <!-- Undo / Redo -->
  <div class="toolbar-group">
    <button class="toolbar-btn" onclick={() => run((v) => undo(v))} title="Undo (Cmd+Z)">
      {@render icon('<path d="M3 7v6h6" /><path d="M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13" />')}
    </button>
    <button class="toolbar-btn" onclick={() => run((v) => redo(v))} title="Redo (Cmd+Shift+Z)">
      {@render icon('<path d="M21 7v6h-6" /><path d="M3 17a9 9 0 0 1 9-9 9 9 0 0 1 6 2.3L21 13" />')}
    </button>
  </div>
  <div class="toolbar-separator"></div>

  <!-- Text formatting -->
  <div class="toolbar-group">
    <button class="toolbar-btn" onclick={() => run(toggleBold)} title="Bold (Cmd+B)">
      {@render icon('<path d="M6 4h8a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z M6 12h9a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z" />')}
    </button>
    <button class="toolbar-btn" onclick={() => run(toggleItalic)} title="Italic (Cmd+I)">
      {@render icon('<line x1="19" y1="4" x2="10" y2="4" /><line x1="14" y1="20" x2="5" y2="20" /><line x1="15" y1="4" x2="9" y2="20" />')}
    </button>
    <button class="toolbar-btn" onclick={() => run(toggleStrikethrough)} title="Strikethrough">
      {@render icon('<path d="M16 4H9a3 3 0 0 0-2.83 4" /><path d="M14 12a4 4 0 0 1 0 8H6" /><line x1="4" y1="12" x2="20" y2="12" />')}
    </button>
    <button class="toolbar-btn" onclick={() => run(toggleInlineCode)} title="Inline Code">
      {@render icon('<polyline points="16 18 22 12 16 6" /><polyline points="8 6 2 12 8 18" />')}
    </button>
  </div>
  <div class="toolbar-separator"></div>

  <!-- Headings -->
  <div class="toolbar-group">
    <button class="toolbar-btn" onclick={() => run((v) => insertHeading(v, 1))} title="Heading 1">
      <span class="toolbar-text">H<sub>1</sub></span>
    </button>
    <button class="toolbar-btn" onclick={() => run((v) => insertHeading(v, 2))} title="Heading 2">
      <span class="toolbar-text">H<sub>2</sub></span>
    </button>
    <button class="toolbar-btn" onclick={() => run((v) => insertHeading(v, 3))} title="Heading 3">
      <span class="toolbar-text">H<sub>3</sub></span>
    </button>
  </div>
  <div class="toolbar-separator"></div>

  <!-- Insert elements -->
  <div class="toolbar-group">
    <button class="toolbar-btn" onclick={() => run(insertLink)} title="Link">
      {@render icon('<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" /><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />')}
    </button>
    <button class="toolbar-btn" onclick={() => run(insertImage)} title="Image">
      {@render icon('<rect x="3" y="3" width="18" height="18" rx="2" ry="2" /><circle cx="8.5" cy="8.5" r="1.5" /><polyline points="21 15 16 10 5 21" />')}
    </button>
    <button class="toolbar-btn" onclick={() => run(insertTable)} title="Table">
      {@render icon('<rect x="3" y="3" width="18" height="18" rx="2" /><line x1="3" y1="9" x2="21" y2="9" /><line x1="3" y1="15" x2="21" y2="15" /><line x1="9" y1="3" x2="9" y2="21" /><line x1="15" y1="3" x2="15" y2="21" />')}
    </button>
  </div>
  <div class="toolbar-separator"></div>

  <!-- Blocks -->
  <div class="toolbar-group">
    <button class="toolbar-btn" onclick={() => run(insertBlockquote)} title="Blockquote">
      {@render icon('<path d="M3 21c3 0 7-1 7-8V5c0-1.25-.756-2.017-2-2H4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2 1 0 1 0 1 1v1c0 1-1 2-2 2s-1 .008-1 1.031V21z" /><path d="M15 21c3 0 7-1 7-8V5c0-1.25-.757-2.017-2-2h-4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2h.75c0 2.25.25 4-2.75 4v3c0 1 0 1 1 1z" />')}
    </button>
    <button class="toolbar-btn" onclick={() => run(insertCodeBlock)} title="Code Block">
      {@render icon('<rect x="3" y="3" width="18" height="18" rx="2" /><path d="M9 9l-2 3 2 3" /><path d="M15 9l2 3-2 3" />')}
    </button>
    <button class="toolbar-btn" onclick={() => run(insertHorizontalRule)} title="Horizontal Rule">
      {@render icon('<line x1="2" y1="12" x2="22" y2="12" />')}
    </button>
  </div>
  <div class="toolbar-separator"></div>

  <!-- Lists -->
  <div class="toolbar-group">
    <button class="toolbar-btn" onclick={() => run(insertBulletList)} title="Bullet List">
      {@render icon('<line x1="8" y1="6" x2="21" y2="6" /><line x1="8" y1="12" x2="21" y2="12" /><line x1="8" y1="18" x2="21" y2="18" /><line x1="3" y1="6" x2="3.01" y2="6" /><line x1="3" y1="12" x2="3.01" y2="12" /><line x1="3" y1="18" x2="3.01" y2="18" />')}
    </button>
    <button class="toolbar-btn" onclick={() => run(insertNumberedList)} title="Numbered List">
      {@render icon('<line x1="10" y1="6" x2="21" y2="6" /><line x1="10" y1="12" x2="21" y2="12" /><line x1="10" y1="18" x2="21" y2="18" /><path d="M4 6h1v4 M4 10h2" /><path d="M6 18H4c0-1 2-2 2-3s-1-1.5-2-1" />')}
    </button>
    <button class="toolbar-btn" onclick={() => run(insertTaskList)} title="Task List">
      {@render icon('<rect x="3" y="5" width="6" height="6" rx="1" /><path d="M3.5 14h.01" /><line x1="13" y1="8" x2="21" y2="8" /><line x1="13" y1="14" x2="21" y2="14" /><rect x="3" y="17" width="6" height="2" rx="1" fill="none" /><path d="M5 11l1 1 2-2" />')}
    </button>
  </div>
</div>
