<script lang="ts">
  // Static markdown reference. Mirrors the toolbar commands in
  // lib/editor/formatting.ts so the panel doubles as a key to those buttons.
  // No props/state — pure reference content; the live Preview tab is the
  // real teacher (type it, see it), this is the glanceable lookup.
  type Row = { syntax: string; label: string };
  type Group = { title: string; rows: Row[] };

  const GROUPS: Group[] = [
    {
      title: "Text",
      rows: [
        { syntax: "**bold**", label: "Bold" },
        { syntax: "*italic*", label: "Italic" },
        { syntax: "~~strike~~", label: "Strikethrough" },
        { syntax: "`code`", label: "Inline code" },
      ],
    },
    {
      title: "Headings",
      rows: [
        { syntax: "# Heading 1", label: "Largest heading" },
        { syntax: "## Heading 2", label: "Section heading" },
        { syntax: "### Heading 3", label: "Sub-heading" },
      ],
    },
    {
      title: "Lists",
      rows: [
        { syntax: "- item", label: "Bulleted list" },
        { syntax: "1. item", label: "Numbered list" },
        { syntax: "- [ ] task", label: "Task checkbox" },
      ],
    },
    {
      title: "Blocks",
      rows: [
        { syntax: "> quote", label: "Blockquote" },
        { syntax: "```\ncode\n```", label: "Code block" },
        { syntax: "---", label: "Horizontal rule" },
      ],
    },
    {
      title: "Links & media",
      rows: [
        { syntax: "[text](url)", label: "Link" },
        { syntax: "![alt](url)", label: "Image" },
      ],
    },
    {
      title: "Tables",
      rows: [
        {
          syntax: "| A | B |\n| --- | --- |\n| 1 | 2 |",
          label: "Table — header, divider, then rows",
        },
      ],
    },
  ];
</script>

<div class="cheatsheet">
  <p class="cheatsheet-tip">
    New to markdown? Type any of these and watch the <strong>Preview</strong> tab
    render it live.
  </p>

  {#each GROUPS as group (group.title)}
    <section class="cheatsheet-group">
      <h3 class="cheatsheet-group-title">{group.title}</h3>
      <dl class="cheatsheet-rows">
        {#each group.rows as row (row.syntax)}
          <div class="cheatsheet-row">
            <dt><code>{row.syntax}</code></dt>
            <dd>{row.label}</dd>
          </div>
        {/each}
      </dl>
    </section>
  {/each}
</div>

<style>
  .cheatsheet {
    padding: 16px;
    font-family: var(--font-sans);
    color: var(--topbar-text);
  }

  .cheatsheet-tip {
    margin: 0 0 20px;
    padding: 10px 12px;
    background: var(--accent-surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-secondary);
  }

  .cheatsheet-group {
    margin-bottom: 20px;
  }

  .cheatsheet-group-title {
    margin: 0 0 8px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--sidebar-muted);
  }

  .cheatsheet-rows {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .cheatsheet-row {
    display: grid;
    grid-template-columns: minmax(0, auto) 1fr;
    align-items: baseline;
    gap: 12px;
  }

  .cheatsheet-row dt {
    min-width: 0;
  }

  .cheatsheet-row dd {
    margin: 0;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .cheatsheet-row code {
    font-family: var(--font-mono);
    font-size: 11px;
    white-space: pre;
    background: var(--preview-code-inline-bg);
    color: var(--preview-code-inline-color);
    padding: 2px 6px;
    border-radius: 3px;
  }
</style>
