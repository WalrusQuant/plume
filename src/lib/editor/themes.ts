import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags } from "@lezer/highlight";
import type { Extension } from "@codemirror/state";

export type Theme = "dark" | "light";

const baseThemeConfig = {
  ".cm-scroller": {
    overflow: "auto",
    fontFamily: "'IBM Plex Mono', 'JetBrains Mono', 'SF Mono', monospace",
    fontSize: "13.5px",
    lineHeight: "1.6",
  },
  ".cm-content": { padding: "12px 0" },
  ".cm-line": { padding: "0 16px" },
  ".cm-cursor": { borderLeftWidth: "1.5px" },
  ".cm-gutters": { border: "none", paddingRight: "6px", fontSize: "12px" },
  ".cm-activeLineGutter": { backgroundColor: "transparent" },
  ".cm-foldGutter": { padding: "0 4px" },
};

const darkTheme = EditorView.theme(
  {
    "&": { height: "100%", backgroundColor: "#1e1e1e", color: "#d4d4d4" },
    ...baseThemeConfig,
    ".cm-content": { ...baseThemeConfig[".cm-content"], caretColor: "#aeafad" },
    ".cm-cursor": { ...baseThemeConfig[".cm-cursor"], borderLeftColor: "#aeafad" },
    ".cm-gutters": { ...baseThemeConfig[".cm-gutters"], backgroundColor: "#1e1e1e", color: "#555555" },
    ".cm-activeLineGutter": { backgroundColor: "transparent", color: "#888888" },
    ".cm-activeLine": { backgroundColor: "rgba(255, 255, 255, 0.03)" },
    ".cm-selectionBackground": { backgroundColor: "rgba(74, 158, 255, 0.15) !important" },
    "&.cm-focused .cm-selectionBackground": { backgroundColor: "rgba(74, 158, 255, 0.2) !important" },
    ".cm-matchingBracket": { backgroundColor: "rgba(255, 255, 255, 0.1)", outline: "1px solid rgba(255, 255, 255, 0.15)" },
    ".cm-foldPlaceholder": { backgroundColor: "#2a2a2a", border: "none", color: "#888888" },
    ".cm-tooltip": { backgroundColor: "#2a2a2a", border: "1px solid #3a3a3a", color: "#d4d4d4" },
    ".cm-panels": { backgroundColor: "#1a1a1a", color: "#d4d4d4" },
    ".cm-searchMatch": { backgroundColor: "rgba(255, 200, 50, 0.2)", outline: "1px solid rgba(255, 200, 50, 0.35)" },
  },
  { dark: true },
);

const lightTheme = EditorView.theme({
  "&": { height: "100%", backgroundColor: "#ffffff", color: "#1f1f1f" },
  ...baseThemeConfig,
  ".cm-content": { ...baseThemeConfig[".cm-content"], caretColor: "#1a1a1a" },
  ".cm-cursor": { ...baseThemeConfig[".cm-cursor"], borderLeftColor: "#1a1a1a" },
  ".cm-gutters": { ...baseThemeConfig[".cm-gutters"], backgroundColor: "#ffffff", color: "#bbbbbb" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "#888888" },
  ".cm-activeLine": { backgroundColor: "rgba(0, 0, 0, 0.02)" },
  ".cm-selectionBackground": { backgroundColor: "rgba(9, 105, 218, 0.1) !important" },
  "&.cm-focused .cm-selectionBackground": { backgroundColor: "rgba(9, 105, 218, 0.15) !important" },
  ".cm-matchingBracket": { backgroundColor: "rgba(0, 0, 0, 0.06)", outline: "1px solid rgba(0, 0, 0, 0.1)" },
  ".cm-foldPlaceholder": { backgroundColor: "#f4f4f4", border: "1px solid #e5e5e5", color: "#999999" },
  ".cm-tooltip": { backgroundColor: "#ffffff", border: "1px solid #e5e5e5", color: "#1f1f1f", boxShadow: "0 2px 8px rgba(0,0,0,0.08)" },
  ".cm-panels": { backgroundColor: "#f9f9f9", color: "#1f1f1f" },
  ".cm-searchMatch": { backgroundColor: "rgba(255, 200, 50, 0.3)", outline: "1px solid rgba(255, 200, 50, 0.5)" },
});

/* Muted, professional syntax colors — VS Code style */
const darkHL = syntaxHighlighting(
  HighlightStyle.define([
    { tag: tags.heading1, color: "#e0e0e0", fontWeight: "700", fontSize: "1.3em" },
    { tag: tags.heading2, color: "#d4d4d4", fontWeight: "600", fontSize: "1.15em" },
    { tag: tags.heading3, color: "#cccccc", fontWeight: "600", fontSize: "1.05em" },
    { tag: tags.heading4, color: "#cccccc", fontWeight: "600" },
    { tag: tags.emphasis, color: "#c5c5c5", fontStyle: "italic" },
    { tag: tags.strong, color: "#e0e0e0", fontWeight: "700" },
    { tag: tags.link, color: "#4a9eff", textDecoration: "underline" },
    { tag: tags.url, color: "#4a9eff" },
    { tag: tags.monospace, color: "#9cdcfe", fontFamily: "inherit" },
    { tag: tags.quote, color: "#888888", fontStyle: "italic" },
    { tag: tags.strikethrough, textDecoration: "line-through", color: "#666666" },
    { tag: tags.meta, color: "#555555" },
    { tag: tags.processingInstruction, color: "#555555" },
    { tag: tags.comment, color: "#555555" },
    { tag: tags.keyword, color: "#569cd6" },
    { tag: tags.string, color: "#6a9955" },
    { tag: tags.number, color: "#b5cea8" },
    { tag: tags.operator, color: "#888888" },
    { tag: tags.punctuation, color: "#666666" },
    { tag: tags.contentSeparator, color: "#444444" },
  ]),
);

const lightHL = syntaxHighlighting(
  HighlightStyle.define([
    { tag: tags.heading1, color: "#111111", fontWeight: "700", fontSize: "1.3em" },
    { tag: tags.heading2, color: "#1a1a1a", fontWeight: "600", fontSize: "1.15em" },
    { tag: tags.heading3, color: "#333333", fontWeight: "600", fontSize: "1.05em" },
    { tag: tags.heading4, color: "#333333", fontWeight: "600" },
    { tag: tags.emphasis, color: "#555555", fontStyle: "italic" },
    { tag: tags.strong, color: "#111111", fontWeight: "700" },
    { tag: tags.link, color: "#0969da", textDecoration: "underline" },
    { tag: tags.url, color: "#0969da" },
    { tag: tags.monospace, color: "#0550ae", fontFamily: "inherit" },
    { tag: tags.quote, color: "#888888", fontStyle: "italic" },
    { tag: tags.strikethrough, textDecoration: "line-through", color: "#bbbbbb" },
    { tag: tags.meta, color: "#999999" },
    { tag: tags.processingInstruction, color: "#999999" },
    { tag: tags.comment, color: "#999999" },
    { tag: tags.keyword, color: "#0550ae" },
    { tag: tags.string, color: "#116329" },
    { tag: tags.number, color: "#0550ae" },
    { tag: tags.operator, color: "#888888" },
    { tag: tags.punctuation, color: "#999999" },
    { tag: tags.contentSeparator, color: "#dddddd" },
  ]),
);

export function themeExtensions(theme: Theme): Extension[] {
  return theme === "dark" ? [darkTheme, darkHL] : [lightTheme, lightHL];
}
