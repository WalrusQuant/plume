<script lang="ts">
  import { onMount } from "svelte";
  import { EditorView, basicSetup } from "codemirror";
  import { keymap } from "@codemirror/view";
  import { indentWithTab } from "@codemirror/commands";
  import { EditorState, Compartment, Prec } from "@codemirror/state";
  import { markdown } from "@codemirror/lang-markdown";
  import { languages } from "@codemirror/language-data";
  import { themeExtensions, type Theme } from "$lib/editor/themes";
  import { toggleBold, toggleItalic, insertLink, toggleInlineCode } from "$lib/editor/formatting";
  import { inlineEditExtension } from "$lib/inlineEdit.svelte";

  const formattingKeymap = Prec.high(
    keymap.of([
      { key: "Mod-b", run: (v) => (toggleBold(v), true) },
      { key: "Mod-i", run: (v) => (toggleItalic(v), true) },
      { key: "Mod-k", run: (v) => (insertLink(v), true) },
      { key: "Mod-e", run: (v) => (toggleInlineCode(v), true) },
      indentWithTab,
    ]),
  );

  interface Props {
    /** Initial document text; the editor owns the text after mount.
        Remount (e.g. {#key docId}) to load a different document. */
    content: string;
    theme: Theme;
    onContentChange: (content: string) => void;
    onEditorReady?: (view: EditorView) => void;
    onCursorChange?: (pos: { line: number; col: number }) => void;
  }

  let { content, theme, onContentChange, onEditorReady, onCursorChange }: Props = $props();

  let container: HTMLDivElement;
  let view: EditorView | null = null;
  const themeComp = new Compartment();

  onMount(() => {
    const state = EditorState.create({
      doc: content,
      extensions: [
        basicSetup,
        formattingKeymap,
        inlineEditExtension,
        EditorView.lineWrapping,
        themeComp.of(themeExtensions(theme)),
        markdown({ codeLanguages: languages }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onContentChange(update.state.doc.toString());
          }
          if (update.selectionSet || update.docChanged) {
            const head = update.state.selection.main.head;
            const line = update.state.doc.lineAt(head);
            onCursorChange?.({ line: line.number, col: head - line.from + 1 });
          }
        }),
      ],
    });
    view = new EditorView({ state, parent: container });
    onEditorReady?.(view);
    return () => {
      view?.destroy();
      view = null;
    };
  });

  // Switch theme dynamically without destroying the editor
  $effect(() => {
    view?.dispatch({ effects: themeComp.reconfigure(themeExtensions(theme)) });
  });
</script>

<div bind:this={container} class="editor-wrapper"></div>
