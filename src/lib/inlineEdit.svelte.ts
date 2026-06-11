import {
  Decoration,
  EditorView,
  WidgetType,
  keymap,
  showTooltip,
  type DecorationSet,
  type Tooltip,
} from "@codemirror/view";
import {
  EditorState,
  Prec,
  StateEffect,
  StateField,
  type Extension,
} from "@codemirror/state";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "$lib/api";
import { assistant } from "$lib/assistant.svelte";
import { toast } from "$lib/toast.svelte";

// ---------------------------------------------------------------------------
// Inline AI edit — select text → menu (Rewrite/Shorten/Expand/Custom) →
// streamed replacement previewed in place → Accept / Reject.
//
// One CM6 file: a StateField holds the edit state, decorations dim the original
// and stream the replacement in a block widget, a tooltip renders the menu, and
// a readOnly facet derived from the field locks the editor while an edit is
// active. The controller singleton drives it and owns the Tauri event plumbing.
// Reuses the chat `assistant:*` events (filtered by stream id) — see
// ai.rs::start_inline_stream.
// ---------------------------------------------------------------------------

// The menu shows whenever there's a non-empty selection and we're idle, so it
// needs no phase of its own — only the active edit does.
type Phase = "idle" | "streaming" | "review";

interface IEState {
  phase: Phase;
  from: number;
  to: number;
  streamed: string;
}

const IDLE: IEState = { phase: "idle", from: 0, to: 0, streamed: "" };

/** Patch the edit state; `null` resets to idle. */
const setState = StateEffect.define<Partial<IEState> | null>();

const ieField = StateField.define<IEState>({
  create: () => IDLE,
  update(value, tr) {
    let v = value;
    // any doc change while active — our own Accept dispatch, History →
    // Restore, or the chat panel's Insert/Replace — cancels the edit
    // (cancelOnDocChange notifies the controller to abort a live stream).
    if (tr.docChanged && v.phase !== "idle") v = IDLE;
    for (const e of tr.effects) {
      if (e.is(setState)) v = e.value ? { ...v, ...e.value } : IDLE;
    }
    return v;
  },
});

/** Locked while streaming/reviewing so the user can't edit the pending range.
    Derived from the field (not a compartment) so EVERY path that resets the
    field — accept, reject, or an external doc change — also unlocks; there is
    no separate unlock step to forget. */
const readOnlyWhileActive = EditorState.readOnly.from(ieField, (v) => v.phase !== "idle");

/** A doc change while an edit is active resets ieField to IDLE (the field's
    own update handles that). readOnly blocks typing but not programmatic
    dispatches — History → Restore and the chat panel's Insert / Replace still
    land — so tell the controller to abort an in-flight stream instead of
    leaving it orphaned. */
const cancelOnDocChange = EditorView.updateListener.of((update) => {
  if (update.docChanged && update.startState.field(ieField).phase !== "idle") {
    inlineEdit.onDocChanged();
  }
});

// ----- menu tooltip --------------------------------------------------------

const ACTIONS = [
  { label: "Rewrite", instruction: "Rewrite the selected text to improve clarity and flow, keeping the same meaning." },
  { label: "Shorten", instruction: "Make the selected text more concise without losing key meaning." },
  { label: "Expand", instruction: "Expand the selected text with more detail and supporting points." },
] as const;

function button(label: string, cls: string, onClick: () => void): HTMLButtonElement {
  const b = document.createElement("button");
  b.className = cls;
  b.textContent = label;
  b.onmousedown = (e) => e.preventDefault(); // keep the editor selection
  b.onclick = onClick;
  return b;
}

function buildMenuTooltip(pos: number): Tooltip {
  return {
    pos,
    above: true,
    create: (view) => {
      const dom = document.createElement("div");
      dom.className = "cm-ie-menu";
      for (const a of ACTIONS) {
        dom.appendChild(
          button(a.label, "cm-ie-menu-btn", () => inlineEdit.runAction(view, a.instruction)),
        );
      }
      const form = document.createElement("form");
      form.className = "cm-ie-custom";
      form.style.display = "none";
      const input = document.createElement("input");
      input.type = "text";
      input.className = "cm-ie-input";
      input.placeholder = "Describe the edit…";
      form.appendChild(input);
      form.onsubmit = (e) => {
        e.preventDefault();
        const v = input.value.trim();
        if (v) inlineEdit.runAction(view, v);
      };
      dom.appendChild(
        button("Custom…", "cm-ie-menu-btn", () => {
          form.style.display = "flex";
          input.focus();
        }),
      );
      dom.appendChild(form);
      return { dom };
    },
  };
}

/** Show the menu whenever text is selected and no edit is in flight. */
function menuTooltips(state: EditorState): readonly Tooltip[] {
  const sel = state.selection.main;
  if (state.field(ieField).phase !== "idle" || sel.empty) return [];
  return [buildMenuTooltip(sel.from)];
}

const tooltipField = StateField.define<readonly Tooltip[]>({
  create: menuTooltips,
  update(tips, tr) {
    // keep the same tooltip instance when nothing relevant changed, so the
    // Custom… input isn't rebuilt (and refocused away) on every transaction
    if (!tr.docChanged && !tr.selection && !tr.effects.some((e) => e.is(setState))) return tips;
    return menuTooltips(tr.state);
  },
  provide: (f) => showTooltip.computeN([f], (state) => state.field(f)),
});

// ----- preview decorations -------------------------------------------------

class ReplacementWidget extends WidgetType {
  constructor(
    readonly text: string,
    readonly phase: Phase,
  ) {
    super();
  }

  eq(other: ReplacementWidget): boolean {
    return other.text === this.text && other.phase === this.phase;
  }

  toDOM(view: EditorView): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "cm-ie-widget";

    const body = document.createElement("div");
    body.className = "cm-ie-widget-text";
    body.textContent = this.text || (this.phase === "streaming" ? "…" : "");
    wrap.appendChild(body);

    const footer = document.createElement("div");
    footer.className = "cm-ie-widget-footer";
    if (this.phase === "review") {
      footer.appendChild(button("Accept", "cm-ie-accept", () => void inlineEdit.accept(view)));
      footer.appendChild(button("Reject", "cm-ie-reject", () => inlineEdit.reject(view)));
    } else {
      const hint = document.createElement("span");
      hint.className = "cm-ie-hint";
      hint.textContent = "Generating… Esc to cancel";
      footer.appendChild(hint);
    }
    wrap.appendChild(footer);
    return wrap;
  }

  ignoreEvent(): boolean {
    return true; // editor ignores widget events; our button handlers still run
  }
}

function buildDeco(state: EditorState): DecorationSet {
  const ie = state.field(ieField);
  if ((ie.phase === "streaming" || ie.phase === "review") && ie.to > ie.from) {
    const lineEnd = state.doc.lineAt(ie.to).to;
    return Decoration.set(
      [
        Decoration.mark({ class: "cm-ie-dim" }).range(ie.from, ie.to),
        Decoration.widget({
          widget: new ReplacementWidget(ie.streamed, ie.phase),
          side: 1,
          block: true,
        }).range(lineEnd),
      ],
      true,
    );
  }
  return Decoration.none;
}

const decoField = StateField.define<DecorationSet>({
  create: buildDeco,
  update: (_deco, tr) => buildDeco(tr.state),
  provide: (f) => EditorView.decorations.from(f),
});

// ----- keymap --------------------------------------------------------------

// No shortcut to memorize: the menu appears on selection and every action is a
// click. Escape is kept only as the universal "cancel" — it dismisses an
// in-flight/preview edit, same as the Reject button.
const inlineEditKeymap = Prec.high(
  keymap.of([
    {
      key: "Escape",
      run: (view) => {
        if (view.state.field(ieField).phase === "idle") return false;
        inlineEdit.reject(view);
        return true;
      },
    },
  ]),
);

export const inlineEditExtension: Extension = [
  ieField,
  tooltipField,
  decoField,
  readOnlyWhileActive,
  cancelOnDocChange,
  inlineEditKeymap,
];

// ----- controller ----------------------------------------------------------

interface StreamToken {
  id: string;
  text: string;
}
interface StreamDone {
  id: string;
  /** True when the stream was aborted (superseded by another AI action). */
  aborted?: boolean;
}
interface StreamError {
  id: string;
  message: string;
}

class InlineEditController {
  private view: EditorView | null = null;
  private docId: string | null = null;
  private getContent: () => string = () => "";
  private onAccepted: (() => void) | null = null;

  /** Id of the in-flight inline stream; events with any other id are stale. */
  private activeStreamId: string | null = null;
  private streamed = "";
  private unlisteners: UnlistenFn[] = [];
  private listening = false;

  /** Register the Tauri event listeners once. Mirrors AssistantStore.init. */
  async init() {
    if (this.listening) return;
    this.listening = true;
    this.unlisteners = await Promise.all([
      listen<StreamToken>("assistant:token", (e) => {
        if (e.payload.id === this.activeStreamId) this.onToken(e.payload.text);
      }),
      listen<StreamDone>("assistant:done", (e) => {
        if (e.payload.id !== this.activeStreamId) return;
        this.activeStreamId = null;
        if (e.payload.aborted) {
          // superseded by another AI action — the streamed text is truncated,
          // discard it instead of presenting it for review
          this.streamed = "";
          toast.error("Inline edit was cancelled by another AI action.");
          this.view?.dispatch({ effects: setState.of(null) });
          return;
        }
        this.view?.dispatch({ effects: setState.of({ phase: "review" }) });
      }),
      listen<StreamError>("assistant:error", (e) => {
        if (e.payload.id !== this.activeStreamId) return;
        toast.error(`Inline edit error: ${e.payload.message}`);
        this.reject();
      }),
    ]);
  }

  destroy() {
    this.unlisteners.forEach((fn) => fn());
    this.unlisteners = [];
    this.listening = false;
  }

  /** Called on doc load/switch: the snapshot target + a live-content getter. */
  setContext(docId: string | null, getContent: () => string, onAccepted: () => void) {
    if (docId !== this.docId && this.activeStreamId) {
      // abort an in-flight edit; the old editor is being remounted
      this.activeStreamId = null;
      void api.stopAssistant();
    }
    this.docId = docId;
    this.getContent = getContent;
    this.onAccepted = onAccepted;
    this.view = null;
    this.streamed = "";
  }

  /** Run an action on the current selection: stream the replacement, lock the
      editor. Reads the live selection — the menu is shown whenever text is
      selected, so there's no separate "open" step. */
  runAction(view: EditorView, instruction: string) {
    const sel = view.state.selection.main;
    if (sel.empty || view.state.field(ieField).phase !== "idle") return;
    if (!assistant.isConfigured) {
      toast.error("Add an AI API key in Settings to use inline edit.");
      return;
    }
    this.view = view;
    this.streamed = "";
    this.activeStreamId = crypto.randomUUID();
    const selectedText = view.state.sliceDoc(sel.from, sel.to);
    // entering "streaming" also locks the editor via the readOnly facet
    view.dispatch({
      effects: setState.of({ phase: "streaming", from: sel.from, to: sel.to, streamed: "" }),
    });
    // Honor the user's selected model (falls back to the provider's fast
    // Haiku-tier model only when none is set).
    api
      .sendInlineEdit(
        this.activeStreamId,
        assistant.settings.provider,
        assistant.settings.model || null,
        instruction,
        selectedText,
        this.getContent(),
        assistant.settings.voice || null,
      )
      .catch((e) => {
        toast.error(`Inline edit failed: ${e}`);
        this.reject(view);
      });
  }

  private onToken(text: string) {
    this.streamed += text;
    this.view?.dispatch({ effects: setState.of({ streamed: this.streamed }) });
  }

  /** Guards accept() against a double-click: the snapshot await between the
      phase check and the dispatch leaves a window where the field is still
      "review" and a second call would splice the replacement twice. */
  private accepting = false;

  /** Apply the replacement: snapshot, then splice it in via dispatch. */
  async accept(view: EditorView) {
    const ie = view.state.field(ieField);
    if (this.accepting || (ie.phase !== "review" && ie.phase !== "streaming")) return;
    this.accepting = true;
    try {
      const replacement = this.streamed;
      this.activeStreamId = null;
      if (this.docId) {
        try {
          await api.createSnapshot(this.docId, this.getContent(), "ai-edit");
        } catch (e) {
          toast.error(`Snapshot failed: ${e}`);
        }
      }
      // dispatch drives the editor's updateListener (save + preview); the doc
      // change resets ieField to idle, which unlocks via the readOnly facet
      view.dispatch({
        changes: { from: ie.from, to: ie.to, insert: replacement },
        selection: { anchor: ie.from + replacement.length },
      });
      this.streamed = "";
      this.onAccepted?.();
    } finally {
      this.accepting = false;
    }
  }

  /** Discard the preview and abort any stream (the field reset unlocks). */
  reject(view?: EditorView) {
    const v = view ?? this.view;
    const wasStreaming = this.activeStreamId !== null;
    this.activeStreamId = null;
    this.streamed = "";
    if (wasStreaming) void api.stopAssistant();
    v?.dispatch({ effects: setState.of(null) });
  }

  /** The document changed under an active edit (restore, chat Insert/Replace,
      or our own accept). The field has already reset itself; abort a stream
      that would otherwise keep running with nowhere to land. */
  onDocChanged() {
    if (this.activeStreamId) {
      this.activeStreamId = null;
      void api.stopAssistant();
    }
    this.streamed = "";
  }
}

export const inlineEdit = new InlineEditController();
