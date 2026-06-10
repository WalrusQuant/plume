import type { EditorView } from '@codemirror/view'

function wrapSelection(view: EditorView, before: string, after: string) {
  const { from, to } = view.state.selection.main
  const selected = view.state.sliceDoc(from, to)
  view.dispatch({
    changes: { from, to, insert: before + selected + after },
    selection: { anchor: from + before.length, head: to + before.length },
  })
  view.focus()
}

function prefixLine(view: EditorView, prefix: string) {
  const { from } = view.state.selection.main
  const line = view.state.doc.lineAt(from)
  view.dispatch({
    changes: { from: line.from, to: line.from, insert: prefix },
    selection: { anchor: from + prefix.length },
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
    })
  } else {
    view.dispatch({
      changes: { from, insert: '[text](url)' },
      selection: { anchor: from + 1, head: from + 5 },
    })
  }
  view.focus()
}

export function insertCodeBlock(view: EditorView) {
  const { from } = view.state.selection.main
  const insert = '```\n\n```'
  view.dispatch({
    changes: { from, insert },
    selection: { anchor: from + 4 },
  })
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
  })
  view.focus()
}

export function insertImage(view: EditorView) {
  const { from, to } = view.state.selection.main
  const selected = view.state.sliceDoc(from, to)
  if (selected) {
    view.dispatch({
      changes: { from, to, insert: `![${selected}](url)` },
      selection: { anchor: from + selected.length + 3, head: from + selected.length + 6 },
    })
  } else {
    view.dispatch({
      changes: { from, insert: '![alt](url)' },
      selection: { anchor: from + 2, head: from + 5 },
    })
  }
  view.focus()
}

export function insertTable(view: EditorView) {
  const { from } = view.state.selection.main
  const table = '| Column 1 | Column 2 | Column 3 |\n| --- | --- | --- |\n| | | |\n'
  view.dispatch({
    changes: { from, insert: table },
    selection: { anchor: from + table.indexOf('| | |') + 2 },
  })
  view.focus()
}

export function insertTaskList(view: EditorView) {
  prefixLine(view, '- [ ] ')
}
