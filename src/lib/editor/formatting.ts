import type { EditorView } from '@codemirror/view'

/** Undo/redo annotation tag so each toolbar action is one undo step.
    (CodeMirror groups transactions with the same userEvent; the explicit
    annotation keeps multi-step inserts from fragmenting.) */
const USER_EVENT = 'input.toolbar'

function wrapSelection(view: EditorView, before: string, after: string) {
  const { from, to } = view.state.selection.main
  const selected = view.state.sliceDoc(from, to)
  view.dispatch({
    changes: { from, to, insert: before + selected + after },
    selection: { anchor: from + before.length, head: to + before.length },
    userEvent: USER_EVENT,
  })
  view.focus()
}

function prefixLine(view: EditorView, prefix: string) {
  const { from } = view.state.selection.main
  const line = view.state.doc.lineAt(from)
  view.dispatch({
    changes: { from: line.from, to: line.from, insert: prefix },
    selection: { anchor: from + prefix.length },
    userEvent: USER_EVENT,
  })
  view.focus()
}

export function toggleBold(view: EditorView) {
  wrapSelection(view, '**', '**')
}

export function toggleItalic(view: EditorView) {
  wrapSelection(view, '*', '*')
}

export function toggleInlineCode(view: EditorView) {
  wrapSelection(view, '`', '`')
}

export function insertHeading(view: EditorView, level: 1 | 2 | 3) {
  prefixLine(view, '#'.repeat(level) + ' ')
}

export function insertBlockquote(view: EditorView) {
  prefixLine(view, '> ')
}

export function insertBulletList(view: EditorView) {
  prefixLine(view, '- ')
}

export function insertNumberedList(view: EditorView) {
  prefixLine(view, '1. ')
}

export function insertLink(view: EditorView) {
  const { from, to } = view.state.selection.main
  const selected = view.state.sliceDoc(from, to)
  if (selected) {
    view.dispatch({
      changes: { from, to, insert: `[${selected}](url)` },
      selection: { anchor: from + selected.length + 3, head: from + selected.length + 6 },
      userEvent: USER_EVENT,
    })
  } else {
    view.dispatch({
      changes: { from, insert: '[text](url)' },
      selection: { anchor: from + 1, head: from + 5 },
      userEvent: USER_EVENT,
    })
  }
  view.focus()
}

export function insertCodeBlock(view: EditorView) {
  const { from, to } = view.state.selection.main
  const selected = view.state.sliceDoc(from, to)
  // Wrap a non-empty selection in fences; otherwise insert an empty fenced
  // block and place the cursor on the blank line inside it. The previous
  // version inserted at `from` and left a dangling selection ahead of the
  // opening fence — a no-op-looking cursor in the middle of the markup.
  if (selected) {
    const open = '```\n'
    const close = '\n```'
    view.dispatch({
      changes: { from, to, insert: open + selected + close },
      selection: { anchor: from + open.length, head: from + open.length + selected.length },
      userEvent: USER_EVENT,
    })
  } else {
    const insert = '```\n\n```'
    // cursor lands on the blank inner line (index of the '\n```' closer + 1)
    const innerStart = 4 // "```\n" then the cursor
    view.dispatch({
      changes: { from, insert },
      selection: { anchor: from + innerStart },
      userEvent: USER_EVENT,
    })
  }
  view.focus()
}

export function toggleStrikethrough(view: EditorView) {
  wrapSelection(view, '~~', '~~')
}

export function insertHorizontalRule(view: EditorView) {
  const { from } = view.state.selection.main
  const line = view.state.doc.lineAt(from)
  const prefix = line.from === from && line.text === '' ? '' : '\n'
  view.dispatch({
    changes: { from, insert: `${prefix}---\n` },
    selection: { anchor: from + prefix.length + 4 },
    userEvent: USER_EVENT,
  })
  view.focus()
}

export function insertImage(view: EditorView) {
  const { from, to } = view.state.selection.main
  const selected = view.state.sliceDoc(from, to)
  if (selected) {
    view.dispatch({
      changes: { from, to, insert: `![${selected}](url)` },
      selection: { anchor: from + selected.length + 4, head: from + selected.length + 7 },
      userEvent: USER_EVENT,
    })
  } else {
    view.dispatch({
      changes: { from, insert: '![alt](url)' },
      selection: { anchor: from + 2, head: from + 5 },
      userEvent: USER_EVENT,
    })
  }
  view.focus()
}

export function insertTable(view: EditorView) {
  const { from } = view.state.selection.main
  // Two leading rows before the body row; cursor lands in the first body
  // cell so the user can start typing immediately. Offsets are computed
  // from the literal template (the old `indexOf('| | |') + 2` was a magic
  // number that broke silently if the template string ever changed).
  const header = '| Column 1 | Column 2 | Column 3 |\n'
  const separator = '| --- | --- | --- |\n'
  const body = '| | | |\n'
  const table = header + separator + body
  // place the caret inside the first empty body cell, just after "| "
  const caretOffset = header.length + separator.length + 2
  view.dispatch({
    changes: { from, insert: table },
    selection: { anchor: from + caretOffset },
    userEvent: USER_EVENT,
  })
  view.focus()
}

export function insertTaskList(view: EditorView) {
  prefixLine(view, '- [ ] ')
}
