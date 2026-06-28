// First-run welcome document. A real, editable doc (seeded once into an empty
// library by +page.svelte's onMount) that teaches markdown in context: every
// feature below renders in the Preview tab, so the doc IS the demo. Points the
// reader at the Guide tab (cheatsheet) and the Assistant tab.

export const WELCOME_DOC_TITLE = "Welcome to Plume";

// Note: code-fence backticks and the `${name}` inside the JS sample are escaped
// so this template literal stays a plain string.
export const WELCOME_DOC_BODY = `# Welcome to Plume 👋

Plume is a **local-first writing studio**: you write in markdown, an AI partner
helps you draft and edit, and you export to wherever you publish — blog,
newsletter, LinkedIn, or X.

This is a real document. Play with it, rewrite it, or delete it when you're
ready — nothing here is special.

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
- [ ] Publish it

## Links and code

Add a [link to anything](https://example.com), or show a block of code:

\`\`\`js
function greet(name) {
  return \`Hi, \${name}!\`;
}
\`\`\`

## Tables

| Feature          | Where to find it |
| ---------------- | ---------------- |
| Live preview     | Preview tab      |
| Markdown syntax  | Guide tab        |
| AI writing partner | Assistant tab  |
| Version history  | History tab      |

---

## Three tabs worth knowing

- **Preview** — your formatted writing, live as you type.
- **Guide** — a one-screen markdown cheatsheet, handy while you learn.
- **Assistant** — ask the AI to draft, rewrite, or brainstorm with you.

When you're ready to start fresh, hit **New** and pick a template. Happy
writing. ✍️
`;
