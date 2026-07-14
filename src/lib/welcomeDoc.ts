// First-run welcome document. A real, editable doc (seeded once into an empty
// library by +page.svelte's onMount) that teaches markdown in context: every
// feature below renders in the Preview tab, so the doc IS the demo. Points the
// reader at the Guide tab (cheatsheet), the Assistant tab, and the local-search
// model they need to download to search their own notes.

export const WELCOME_DOC_TITLE = "Welcome to Plume";

// Note: code-fence backticks and the `${name}` inside the JS sample are escaped
// so this template literal stays a plain string.
export const WELCOME_DOC_BODY = `# Welcome to Plume 👋

Plume is a **local-first AI writing studio** for people who build in public. You
write in markdown, an AI partner drafts and edits alongside you, and it can
search your own notes — all on your machine. Nothing leaves your computer unless
you send it.

This is a real document. Play with it, rewrite it, or delete it when you're
ready — nothing here is special.

---

## Search your own notes 🔎

Plume can ground the AI in *your* writing. In the **Assistant** tab, turn on
**Search your notes** and Plume pulls the most relevant passages from your own
documents and answers from them — a private research partner over everything
you've written.

This runs a small model **entirely on your machine**, so you download it once
before it works:

1. Open **Settings** (the gear icon).
2. Go to the **Local search** tab.
3. Pick a model and click **Download**. Plume then indexes your notes quietly in
   the background.

You can switch models or remove the download any time — either way, your
documents never leave your computer.

---

## What is markdown?

Markdown formats text with a few plain symbols. You write on the left; the
**Preview** tab on the right shows the finished result. Edit a line and watch
it update. Below are the basics, shown in context.

## Text styling

This sentence has **bold words**, *italic words*, and a bit of \`inline code\`.
You can even ~~cross things out~~.

> Wrap a line in a blockquote for pull-quotes, asides, or callouts like this.

## Lists

A few things markdown does well:

- Bulleted lists for unordered points
- Mixing **styles** inside an item
- Indent to create a sub-point
  - like this one

Steps in order:

1. Type your idea
2. Shape it with the **Assistant** tab
3. Export it from the top bar

And a checklist to track work:

- [x] Open Plume
- [ ] Write something
- [ ] Search your notes

## Links and code

Add a [link to anything](https://example.com), or show a block of code:

\`\`\`js
function greet(name) {
  return \`Hi, \${name}!\`;
}
\`\`\`

## Tables

| Feature            | Where to find it  |
| ------------------ | ----------------- |
| Live preview       | Preview tab       |
| Markdown syntax    | Guide tab         |
| AI writing partner | Assistant tab     |
| Search your notes  | Settings → Local search |
| Version history    | History tab       |

---

## The tabs worth knowing

- **Preview** — your formatted writing, live as you type.
- **Guide** — a one-screen markdown cheatsheet, handy while you learn.
- **Assistant** — draft, rewrite, or brainstorm with the AI, and search your notes.
- **History** — snapshots of your document you can restore any time.

When you're ready to start fresh, hit **New** and pick a template. Happy
writing. ✍️
`;
