<script lang="ts">
  import type { EditorView } from "@codemirror/view";
  import {
    toggleBold,
    toggleItalic,
    toggleStrikethrough,
    toggleInlineCode,
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

  type Item = {
    id: string;
    syntax: string;
    label: string;
    how: string;
    apply: (view: EditorView) => void;
  };

  type Group = { title: string; items: Item[] };

  const GROUPS: Group[] = [
    {
      title: "Text",
      items: [
        { id: "bold", syntax: "**bold**", label: "Bold", how: "Two asterisks on each side", apply: toggleBold },
        { id: "italic", syntax: "*italic*", label: "Italic", how: "One asterisk on each side", apply: toggleItalic },
        { id: "strike", syntax: "~~strike~~", label: "Strikethrough", how: "Tildes wrap the text", apply: toggleStrikethrough },
        { id: "code", syntax: "`code`", label: "Inline code", how: "Backticks around a short snippet", apply: toggleInlineCode },
      ],
    },
    {
      title: "Headings",
      items: [
        { id: "h1", syntax: "# Heading 1", label: "Largest heading", how: "One hash, then a space", apply: (v) => insertHeading(v, 1) },
        { id: "h2", syntax: "## Heading 2", label: "Section heading", how: "Two hashes", apply: (v) => insertHeading(v, 2) },
        { id: "h3", syntax: "### Heading 3", label: "Sub-heading", how: "Three hashes", apply: (v) => insertHeading(v, 3) },
      ],
    },
    {
      title: "Lists",
      items: [
        { id: "ul", syntax: "- item", label: "Bulleted list", how: "Dash, then a space", apply: insertBulletList },
        { id: "ol", syntax: "1. item", label: "Numbered list", how: "Number, period, space", apply: insertNumberedList },
        { id: "task", syntax: "- [ ] task", label: "Task checkbox", how: "A list item with [ ] for unchecked", apply: insertTaskList },
      ],
    },
    {
      title: "Blocks",
      items: [
        { id: "quote", syntax: "> quote", label: "Blockquote", how: "Greater-than at the start of the line", apply: insertBlockquote },
        {
          id: "fence",
          syntax: "```\ncode\n```",
          label: "Code block",
          how: "Three backticks to open and close. A language name can follow the first ```",
          apply: insertCodeBlock,
        },
        { id: "hr", syntax: "---", label: "Horizontal rule", how: "Three dashes on their own line", apply: insertHorizontalRule },
      ],
    },
    {
      title: "Links & media",
      items: [
        { id: "link", syntax: "[text](url)", label: "Link", how: "Label in brackets, address in parentheses", apply: insertLink },
        { id: "image", syntax: "![alt](url)", label: "Image", how: "Same as a link, with a leading !", apply: insertImage },
      ],
    },
    {
      title: "Tables",
      items: [
        {
          id: "table",
          syntax: "| Header | Header |\n| --- | --- |\n| cell | cell |",
          label: "Table",
          how: "Header row, a divider of dashes, then body rows",
          apply: insertTable,
        },
      ],
    },
  ];

  let justInserted = $state<string | null>(null);
  let flashTimer: ReturnType<typeof setTimeout> | undefined;

  function insert(item: Item) {
    if (!editorView) return;
    item.apply(editorView);
    justInserted = item.id;
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => {
      if (justInserted === item.id) justInserted = null;
    }, 1100);
  }
</script>

<div class="guide">
  <p class="guide-lede">
    Click a row to insert it at the cursor. Open <strong>Preview</strong> to see it render.
  </p>

  {#each GROUPS as group (group.title)}
    <section class="guide-group">
      <h3 class="guide-group-title">{group.title}</h3>
      <div class="guide-rows">
        {#each group.items as item (item.id)}
          <button
            type="button"
            class="guide-row"
            class:guide-row--flash={justInserted === item.id}
            disabled={!editorView}
            onclick={() => insert(item)}
          >
            <code class="guide-syntax">{item.syntax}</code>
            <span class="guide-copy">
              <span class="guide-label">{justInserted === item.id ? "Inserted" : item.label}</span>
              <span class="guide-how">{item.how}</span>
            </span>
          </button>
        {/each}
      </div>
    </section>
  {/each}
</div>

<style>
  .guide {
    padding: 16px 16px 28px;
    font-family: var(--font-sans);
    color: var(--topbar-text);
  }

  .guide-lede {
    margin: 0 0 20px;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text-secondary);
  }

  .guide-group {
    margin-bottom: 18px;
  }

  .guide-group-title {
    margin: 0 0 6px;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-tertiary);
  }

  .guide-rows {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .guide-row {
    display: grid;
    grid-template-columns: minmax(7.5rem, 38%) 1fr;
    align-items: start;
    gap: 12px 16px;
    width: 100%;
    padding: 8px 10px;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 6px;
    color: inherit;
    cursor: pointer;
    transition: background var(--transition), border-color var(--transition);
  }

  .guide-row:hover:not(:disabled) {
    background: var(--sidebar-hover);
    border-color: var(--border);
  }

  .guide-row:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .guide-row--flash {
    background: var(--accent-surface);
    border-color: var(--accent);
  }

  .guide-syntax {
    display: block;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: 11.5px;
    line-height: 1.45;
    color: var(--preview-code-inline-color);
    background: var(--preview-code-inline-bg);
    padding: 4px 7px;
    border-radius: 4px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .guide-copy {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    padding-top: 3px;
  }

  .guide-label {
    font-size: 13px;
    font-weight: 500;
    color: var(--preview-text);
  }

  .guide-row--flash .guide-label {
    color: var(--accent);
  }

  .guide-how {
    font-size: 12px;
    line-height: 1.4;
    color: var(--text-secondary);
  }
</style>
