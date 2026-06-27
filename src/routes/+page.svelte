<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { EditorView } from "@codemirror/view";
  import { api, type DocType, type Document, type Folder } from "$lib/api";
  import { getTemplate } from "$lib/templates";
  import type { Theme } from "$lib/editor/themes";
  import Editor from "$lib/components/Editor.svelte";
  import Toolbar from "$lib/components/Toolbar.svelte";
  import TopBar from "$lib/components/TopBar.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import HomeShelf from "$lib/components/HomeShelf.svelte";
  import Preview from "$lib/components/Preview.svelte";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import NewDocumentDialog from "$lib/components/NewDocumentDialog.svelte";
  import IdeaCaptureModal from "$lib/components/IdeaCaptureModal.svelte";
  import MultiplyModal from "$lib/components/MultiplyModal.svelte";
  import { type MultiplyProgress, type MultiplyTarget } from "$lib/multiplyTargets";
  import RightPaneTabs, { type RightPaneTab } from "$lib/components/RightPaneTabs.svelte";
  import AssistantPanel from "$lib/components/AssistantPanel.svelte";
  import HistoryPanel from "$lib/components/HistoryPanel.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import { assistant } from "$lib/assistant.svelte";
  import { inlineEdit } from "$lib/inlineEdit.svelte";
  import { ideaExpand } from "$lib/ideaExpand.svelte";
  import { multiply } from "$lib/multiply.svelte";
  import { aiBusy } from "$lib/aiBusy.svelte";
  import { toast } from "$lib/toast.svelte";
  import { formatError } from "$lib/formatError";
  import type { SnapshotMeta } from "$lib/api";

  const SAVE_DEBOUNCE_MS = 500;
  const PREVIEW_DEBOUNCE_MS = 150;
  /** How often active editing produces an automatic version snapshot. */
  const SNAPSHOT_INTERVAL_MS = 10 * 60 * 1000;
  const THEME_KEY = "markdown-theme";
  const FOCUS_KEY = "markdown-focus-mode";

  let documents = $state<Document[]>([]);
  let folders = $state<Folder[]>([]);
  let selectedDocId = $state<string | null>(null);
  /** Live text of the selected doc. Seeds the editor on remount ({#key});
      after that the editor owns the text and reports changes back here. */
  let content = $state("");
  let loading = $state(true);

  let theme = $state<Theme>("dark");
  let sidebarCollapsed = $state(false);
  /** Focus mode: hide the right pane (preview/assistant/history) for a
      distraction-free, full-width editor. Persisted across sessions. */
  let focusMode = $state(false);
  let dialogOpen = $state(false);
  /** Type pre-selected in the new-document dialog (e.g. "plan" from the shelf). */
  let dialogInitialType = $state<DocType>("generic");
  /** Project the dialog's new doc should land in; null = unfiled. */
  let dialogFolderId = $state<string | null>(null);
  let settingsOpen = $state(false);
  type PreviewMode = "rendered" | "linkedin" | "x-thread" | "x-article";
  let previewMode = $state<PreviewMode>("rendered");
  let linkedinText = $state("");
  let xThreadText = $state("");
  let xArticleHtml = $state("");

  /** Fire-and-forget with a visible error toast on failure. */
  function run(promise: Promise<unknown>, what: string) {
    promise.catch((e) => toast.error(`${what} failed: ${formatError(e)}`));
  }

  let editorView = $state<EditorView | null>(null);
  let cursorPos = $state({ line: 1, col: 1 });
  let previewHtml = $state("");
  let wordCount = $state(0);
  let rightTab = $state<RightPaneTab>("preview");
  let exportTargets = $state<import("$lib/api").ExportTarget[]>([]);
  let exportStatus = $state("");
  let exportStatusTimer: ReturnType<typeof setTimeout> | null = null;

  // ----- version snapshots (history) -----
  let snapshots = $state<SnapshotMeta[]>([]);
  /** Last automatic-snapshot time per document, to throttle interval captures. */
  const lastSnapshotAt = new Map<string, number>();

  const selectedDoc = $derived(documents.find((d) => d.id === selectedDocId));
  /** On home the shelf IS the navigation — the sidebar only shows with a doc. */
  const isHome = $derived(!loading && !selectedDoc);

  // ----- persistence (debounced save) -----

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  /** Unsaved content per document. Failed saves stay here until they land,
      so switching documents (and typing there) can't clobber them. */
  const pendingSaves = new Map<string, string>();
  /** "saved" | "saving" | "unsaved" | "error" — drives the StatusBar indicator. */
  let saveStatus = $state<"saved" | "saving" | "unsaved" | "error">("saved");

  function scheduleSave(docId: string, content: string) {
    pendingSaves.set(docId, content);
    saveStatus = "unsaved";
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(flushSave, SAVE_DEBOUNCE_MS);
  }

  async function flushSave() {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (pendingSaves.size > 0) saveStatus = "saving";
    let anyError = false;
    for (const [id, content] of [...pendingSaves]) {
      pendingSaves.delete(id);
      try {
        await api.saveDocumentContent(id, content);
      } catch (e) {
        // keep it for the next keystroke/flush to retry — unless newer
        // content for this doc was scheduled while the save was in flight
        if (!pendingSaves.has(id)) pendingSaves.set(id, content);
        anyError = true;
        toast.error(`Saving failed: ${formatError(e)}`);
        continue;
      }
      const doc = documents.find((d) => d.id === id);
      if (doc) doc.updatedAt = new Date().toISOString();
      maybeIntervalSnapshot(id, content);
    }
    if (anyError) saveStatus = "error";
    else if (pendingSaves.size === 0) saveStatus = "saved";
  }

  /** Capture an automatic version once per SNAPSHOT_INTERVAL_MS of editing. */
  function maybeIntervalSnapshot(id: string, content: string) {
    if (Date.now() - (lastSnapshotAt.get(id) ?? 0) < SNAPSHOT_INTERVAL_MS) return;
    lastSnapshotAt.set(id, Date.now());
    void api
      .createSnapshot(id, content, "interval")
      .then((snap) => {
        if (snap && id === selectedDocId && rightTab === "history") void loadSnapshots();
      })
      .catch((e) => toast.error(`Snapshot failed: ${formatError(e)}`));
  }

  // ----- preview (debounced render via comrak) -----

  let previewTimer: ReturnType<typeof setTimeout> | null = null;

  function schedulePreview(content: string) {
    // Right pane is unmounted in focus mode — don't burn IPC rendering a hidden
    // preview. toggleFocusMode re-schedules a render when the pane reappears.
    if (focusMode) return;
    if (previewTimer) clearTimeout(previewTimer);
    previewTimer = setTimeout(() => run(updatePreview(content), "Preview"), PREVIEW_DEBOUNCE_MS);
  }

  async function updatePreview(content: string) {
    if (previewMode === "linkedin") {
      linkedinText = await api.renderLinkedinPreview(content);
    } else if (previewMode === "x-thread") {
      xThreadText = await api.renderXThreadPreview(content);
    } else if (previewMode === "x-article") {
      xArticleHtml = await api.renderXArticlePreview(content);
    } else {
      previewHtml = await api.renderPreview(content);
    }
  }

  function setPreviewMode(mode: PreviewMode) {
    previewMode = mode;
    run(updatePreview(content), "Preview");
  }

  function countWords(content: string): number {
    const words = content.trim().match(/\S+/g);
    return words ? words.length : 0;
  }

  function onContentChange(next: string) {
    content = next;
    if (selectedDocId) scheduleSave(selectedDocId, next);
    schedulePreview(next);
    wordCount = countWords(next);
  }

  async function applyAssistantContent(next: string) {
    if (!editorView || !selectedDocId) return;
    // "Replace document" clobbers the whole draft — capture a safety version
    // first (same as snapshot Restore) so the prior text is recoverable.
    try {
      await api.createSnapshot(selectedDocId, content, "ai-edit");
    } catch (e) {
      toast.error(`Snapshot failed: ${formatError(e)}`);
    }
    // dispatch fires the editor's updateListener, so save + preview follow
    editorView.dispatch({
      changes: { from: 0, to: editorView.state.doc.length, insert: next },
    });
    refreshHistoryIfOpen();
  }

  async function exportTo(targetId: string) {
    if (!selectedDoc) return;
    if (!content.trim()) {
      toast.error("Nothing to export — the document is empty.");
      return;
    }
    await flushSave();
    // file exports render (and decode images) before the save dialog appears —
    // give immediate feedback so a slow docx doesn't look like a frozen click
    showExportStatus("Exporting…", true);
    let result: Awaited<ReturnType<typeof api.exportDocument>>;
    try {
      result = await api.exportDocument(content, selectedDoc.name, targetId);
    } catch (e) {
      // render/build failed — surface via toast like every other failure
      showExportStatus("");
      toast.error(`Export failed: ${formatError(e)}`);
      return;
    }
    try {
      if (result.type === "clipboard") {
        await navigator.clipboard.writeText(result.text);
        showExportStatus("Copied to clipboard — ready to paste");
      } else if (result.type === "clipboardHtml") {
        await navigator.clipboard.write([
          new ClipboardItem({
            "text/html": new Blob([result.html], { type: "text/html" }),
            "text/plain": new Blob([result.plain], { type: "text/plain" }),
          }),
        ]);
        showExportStatus("Copied (rich) — paste into the editor");
      } else if (result.type === "file") {
        showExportStatus(`Saved: ${result.path}`);
      } else if (result.type === "cancelled") {
        showExportStatus("Export canceled");
      }
    } catch (e) {
      // the document rendered fine — only the OS clipboard handoff failed
      showExportStatus("");
      toast.error(`Couldn't copy to the clipboard: ${formatError(e)}`);
    }
  }

  /** Show a transient export status. `sticky` keeps it until the next call
      (used for the "Exporting…" in-progress message, replaced by the result). */
  function showExportStatus(message: string, sticky = false) {
    exportStatus = message;
    if (exportStatusTimer) clearTimeout(exportStatusTimer);
    exportStatusTimer = sticky ? null : setTimeout(() => (exportStatus = ""), 5000);
  }

  function insertAssistantContent(content: string) {
    if (!editorView) return;
    const { from, to } = editorView.state.selection.main;
    editorView.dispatch({
      changes: { from, to, insert: content },
      selection: { anchor: from + content.length },
    });
  }

  // ----- version snapshots -----

  async function loadSnapshots() {
    snapshots = selectedDocId ? await api.listSnapshots(selectedDocId) : [];
  }

  async function saveSnapshot() {
    if (!selectedDocId) return;
    await flushSave();
    const snap = await api.createSnapshot(selectedDocId, content, "manual");
    if (snap) lastSnapshotAt.set(selectedDocId, Date.now());
    await loadSnapshots();
  }

  async function restoreSnapshot(snapshotId: string) {
    if (!selectedDocId || !editorView) return;
    // flush any pending save first so the pre-restore snapshot captures what's
    // actually on disk, not just in-memory text
    await flushSave();
    // safety capture of the current text, then swap in the restored version
    await api.createSnapshot(selectedDocId, content, "restore");
    const restored = await api.getSnapshotContent(snapshotId);
    // dispatch so the editor's updateListener drives save + preview (one-way flow)
    editorView.dispatch({
      changes: { from: 0, to: editorView.state.doc.length, insert: restored },
    });
    await loadSnapshots();
    toast.show("Restored — a pre-restore version was saved to history.", "info");
  }

  function changeRightTab(tab: RightPaneTab) {
    rightTab = tab;
    if (tab === "history") run(loadSnapshots(), "Loading history");
  }

  // ----- document/folder actions -----

  /** Bumped whenever the open document changes (select / home / delete), so a
      slower in-flight content fetch can detect it was superseded and bail —
      mirrors the assistant store's loadSeq guard. */
  let docLoadSeq = 0;
  /** True while a doc's content is being fetched — drives the editor-pane spinner. */
  let docLoading = $state(false);

  async function selectDocument(id: string) {
    if (id === selectedDocId) return;
    await flushSave();
    const seq = ++docLoadSeq;
    docLoading = true;
    try {
      const loaded = await api.getDocumentContent(id);
      if (seq !== docLoadSeq) return; // a newer switch superseded this; it owns the flag
      // Null only once we're committed to the switch: if the fetch above threw,
      // the old doc stays open and its editor must keep working (selectedDocId
      // is unchanged on error, so the {#key} never remounts the Editor).
      editorView = null;
      content = loaded;
      selectedDocId = id;
      schedulePreview(content);
      wordCount = countWords(content);
      void assistant.loadFor(id);
      inlineEdit.setContext(id, () => content, refreshHistoryIfOpen);
      if (rightTab === "history") void loadSnapshots();
    } finally {
      if (seq === docLoadSeq) docLoading = false;
    }
  }

  /** Refresh the history panel after an inline edit lands an ai-edit snapshot. */
  function refreshHistoryIfOpen() {
    if (rightTab === "history") void loadSnapshots();
  }

  async function createDocument(name: string, type: DocType, content?: string) {
    const body = content ?? (getTemplate(type) || undefined);
    const doc = await api.createDocument(name, type, body);
    documents = [doc, ...documents];
    await selectDocument(doc.id);
    return doc;
  }

  /** Open the new-document dialog, optionally pre-selecting a type and a
      destination project (the shelf's "New plan" / per-project "+ New page"). */
  function openNewDocument(type: DocType = "generic", folderId: string | null = null) {
    dialogInitialType = type;
    dialogFolderId = folderId;
    dialogOpen = true;
  }

  async function createDocumentFromDialog(name: string, type: DocType) {
    const doc = await createDocument(name, type);
    if (dialogFolderId) await moveDocument(doc.id, dialogFolderId);
  }

  /** Back to the shelf: no document open. */
  async function goHome() {
    await flushSave();
    docLoadSeq++; // cancel any in-flight document load
    docLoading = false;
    editorView = null;
    selectedDocId = null;
    void assistant.loadFor(null);
  }

  // ----- idea inbox -----
  //
  // Ideas are quick notes, never opened in the big editor. They're captured and
  // edited in a small modal (the open document never changes underneath you),
  // and have three verbs: expand with AI, convert to a document, delete.

  /** Id of the idea currently being expanded — drives the sidebar spinner. */
  let expandingId = $state<string | null>(null);
  /** Target label of the in-flight expansion, e.g. "Blog Post". */
  let expandingLabel = $state("");

  // Capture/edit modal state. A row is created only on save, so abandoning the
  // modal leaves no empty idea behind.
  let ideaModalOpen = $state(false);
  let ideaModalMode = $state<"new" | "edit">("new");
  let ideaModalId = $state<string | null>(null);
  let ideaModalTitle = $state("");
  let ideaModalBody = $state("");

  /** A derived idea title is its first non-empty line (markdown heading marks
      stripped); empty → "New idea". Used only when no explicit title is set. */
  function deriveIdeaName(content: string): string {
    const firstLine = content.split("\n").find((l) => l.trim()) ?? "";
    const cleaned = firstLine.replace(/^#+\s*/, "").trim();
    if (!cleaned) return "New idea";
    // slice by code points so an emoji at the boundary isn't split in half
    const points = [...cleaned];
    return points.length > 40 ? `${points.slice(0, 40).join("")}…` : cleaned;
  }

  /** "+ New idea": open the capture modal. No row is created until save. */
  function newIdea() {
    ideaModalMode = "new";
    ideaModalId = null;
    ideaModalTitle = "";
    ideaModalBody = "";
    ideaModalOpen = true;
  }

  /** Click an idea: load it into the capture modal — without selecting it into
      the big editor. Only seed the title field if it's explicit; seeding a
      derived name would lock it explicit on save and freeze auto-follow. */
  async function openIdea(id: string) {
    const idea = documents.find((d) => d.id === id);
    const body = await api.getDocumentContent(id);
    ideaModalMode = "edit";
    ideaModalId = id;
    ideaModalTitle = idea?.titleExplicit ? idea.name : "";
    ideaModalBody = body;
    ideaModalOpen = true;
  }

  /** Save the capture modal. title === "" means "derive from the first line".
      Bypasses the editor save loop, so the open document is untouched. */
  async function saveIdea(title: string, body: string) {
    const explicit = title.length > 0;
    const name = explicit ? title : deriveIdeaName(body);
    if (ideaModalMode === "new") {
      if (!explicit && !body.trim()) {
        toast.show("Nothing to capture — the idea was empty.", "info");
        return;
      }
      const doc = await api.createDocument(name, "idea", body);
      documents = [doc, ...documents];
      if (explicit) {
        const updated = await api.updateIdeaName(doc.id, name, true);
        documents = documents.map((d) => (d.id === doc.id ? updated : d));
      }
    } else if (ideaModalId) {
      const id = ideaModalId;
      await api.saveDocumentContent(id, body);
      const updated = await api.updateIdeaName(id, name, explicit);
      documents = documents.map((d) => (d.id === id ? updated : d));
    }
  }

  /** Convert an idea into a real document of `type`, as-is (no AI). It leaves
      the Inbox (which filters on type). Stays on the current open document. */
  async function convertIdea(ideaId: string, type: DocType) {
    const idea = documents.find((d) => d.id === ideaId);
    if (!idea) return;
    const body = await api.getDocumentContent(ideaId);
    if (!body.trim()) {
      toast.error("Add some text to the idea before converting it.");
      return;
    }
    const updated = await api.updateDocumentType(ideaId, type, idea.name, true);
    documents = documents.map((d) => (d.id === ideaId ? updated : d));
  }

  /** Expand an idea into a full draft of `type`, then open the draft. The idea
      itself is kept in the Inbox. */
  async function expandIdea(ideaId: string, type: DocType, label: string) {
    if (aiBusy.busy) {
      toast.error(`Wait for the ${aiBusy.label} to finish first.`);
      return;
    }
    const idea = documents.find((d) => d.id === ideaId);
    const ideaText = await api.getDocumentContent(ideaId);
    if (!ideaText.trim()) {
      toast.error("Add some text to the idea before expanding it.");
      return;
    }
    expandingId = ideaId;
    expandingLabel = label;
    aiBusy.begin("idea expansion"); // block other AI actions from stealing the slot
    try {
      const draft = await ideaExpand.expand(ideaText, label);
      const name = idea?.titleExplicit ? `${idea.name} — ${label}` : `${label} draft`;
      await createDocument(name, type, draft);
      toast.show(`Draft created: ${name}`, "info");
    } catch (e) {
      if (!ideaExpand.canceled) throw e; // a cancel is intentional — stay quiet
    } finally {
      aiBusy.end();
      expandingId = null;
      expandingLabel = "";
    }
  }

  /** Cancel a running idea expansion (the Inbox spinner's cancel button). */
  function cancelExpand() {
    ideaExpand.cancel();
  }

  // ----- content multiplication -----
  //
  // From the open source document, generate a platform-native variant for each
  // chosen target, sequentially (the single AiState slot allows only one stream
  // at a time). The source + its variants are collected into one folder — reuse
  // the source's folder if it has one, else create a folder named after it. The
  // editor stays on the source throughout.

  let multiplyOpen = $state(false);
  /** Non-null while/after a multiply run; one entry per chosen target. */
  let multiplyProgress = $state<MultiplyProgress[] | null>(null);
  /** Set when the user cancels a run, so the batch loop stops after the current
      target instead of pressing on to the remaining ones. */
  let multiplyCanceled = false;

  function openMultiply() {
    multiplyProgress = null;
    multiplyCanceled = false;
    multiplyOpen = true;
  }

  /** Cancel a running multiply: stop the active stream and let the loop unwind. */
  function cancelMultiply() {
    multiplyCanceled = true;
    multiply.cancel();
  }

  function closeMultiply() {
    multiplyOpen = false;
    multiplyProgress = null;
  }

  async function multiplyDocument(targets: MultiplyTarget[]) {
    const source = selectedDoc;
    if (!source) return;
    if (aiBusy.busy) {
      toast.error(`Wait for the ${aiBusy.label} to finish first.`);
      return;
    }
    await flushSave(); // multiply the saved source, not stale editor text
    const sourceText = content;
    const baseName = source.name;
    const sourceId = source.id;

    // The target folder is created lazily, on the first draft that succeeds —
    // a run where every generation fails (no network, bad key) must not leave
    // the source relocated into a new folder as a side effect.
    let folderId = source.folderId;
    async function ensureFolder(): Promise<string> {
      if (!folderId) {
        const folder = await api.createFolder(baseName);
        folderId = folder.id;
        await api.moveDocument(sourceId, folderId);
      }
      return folderId;
    }

    multiplyProgress = targets.map((t) => ({ ...t, status: "pending" }));

    // hold the slot for the whole batch (incl. the doc-creation gaps between
    // targets) so an incidental chat/inline/expand can't abort a mid-batch stream
    aiBusy.begin("content multiply");
    let consecutiveFailures = 0;
    try {
      for (let i = 0; i < targets.length; i++) {
        if (multiplyCanceled) break; // leave the rest pending
        multiplyProgress[i].status = "running";
        try {
          // awaited → strictly sequential, honoring the single AiState slot
          const draft = await multiply.generate(sourceText, targets[i].type, targets[i].label);
          const targetFolder = await ensureFolder();
          const doc = await api.createDocument(`${baseName} — ${targets[i].label}`, targets[i].type, draft);
          await api.moveDocument(doc.id, targetFolder);
          multiplyProgress[i].status = "done";
          consecutiveFailures = 0;
        } catch {
          multiplyProgress[i].status = "error";
          // a user cancel rejects the same way — stay quiet and stop the batch
          if (multiplyCanceled) break;
          // the modal's error row already reports it — don't also toast each one.
          // repeated failures mean a systemic problem (key/network), so stop.
          if (++consecutiveFailures >= 2) {
            toast.error("Multiply stopped — several targets failed. Check your AI key and connection.");
            break;
          }
        }
      }
    } finally {
      aiBusy.end();
    }

    // One refresh so the sidebar reflects the new folder, moved source, and variants.
    [documents, folders] = await Promise.all([api.listDocuments(), api.listFolders()]);
  }

  async function renameDocument(id: string, name: string) {
    const updated = await api.renameDocument(id, name);
    documents = documents.map((d) => (d.id === id ? updated : d));
  }

  async function moveDocument(id: string, folderId: string | null) {
    const updated = await api.moveDocument(id, folderId);
    documents = documents.map((d) => (d.id === id ? updated : d));
  }

  /** Persist a manual reorder of one sidebar section. Optimistic: stamp each
      id's new sortOrder (the global `documents` array stays recency-ordered;
      buildSidebarTree re-sorts each section), roll back on failure. */
  async function reorderDocuments(ids: string[]) {
    const prev = documents;
    const orderOf = new Map(ids.map((id, i) => [id, i]));
    documents = documents.map((d) =>
      orderOf.has(d.id) ? { ...d, sortOrder: orderOf.get(d.id)! } : d,
    );
    try {
      await api.reorderDocuments(ids);
    } catch (e) {
      documents = prev; // restore previous order; run() surfaces the toast
      throw e;
    }
  }

  /** Reorder the folders themselves. The `folders` array order IS the sidebar
      order, so rebuild it in drop order (ids covers every folder). */
  async function reorderFolders(ids: string[]) {
    const prev = folders;
    const byId = new Map(folders.map((f) => [f.id, f]));
    folders = ids
      .map((id, i) => {
        const f = byId.get(id);
        return f ? { ...f, sortOrder: i } : null;
      })
      .filter((f): f is Folder => f !== null);
    try {
      await api.reorderFolders(ids);
    } catch (e) {
      folders = prev;
      throw e;
    }
  }

  async function deleteDocument(id: string) {
    const name = documents.find((d) => d.id === id)?.name ?? "Document";
    await api.deleteDocument(id);
    pendingSaves.delete(id); // never retry a save against a deleted row
    lastSnapshotAt.delete(id);
    documents = documents.filter((d) => d.id !== id);
    toast.show(`Deleted “${name}”`, "info");
    // if the deleted idea is open in the capture modal, close it
    if (ideaModalId === id) ideaModalOpen = false;
    if (selectedDocId === id) {
      // land on the home shelf rather than yanking another doc open
      docLoadSeq++; // cancel any in-flight document load
      selectedDocId = null;
      editorView = null;
      void assistant.loadFor(null);
    }
  }

  async function createFolder(name: string): Promise<Folder> {
    try {
      const folder = await api.createFolder(name);
      folders = [...folders, folder];
      return folder;
    } catch (e) {
      toast.error(`Creating folder failed: ${formatError(e)}`);
      throw e;
    }
  }

  async function renameFolder(id: string, name: string) {
    const updated = await api.renameFolder(id, name);
    folders = folders.map((f) => (f.id === id ? updated : f));
  }

  async function toggleFolderActive(id: string, active: boolean) {
    const updated = await api.setFolderActive(id, active);
    folders = folders.map((f) => (f.id === id ? updated : f));
  }

  async function deleteFolder(id: string) {
    await api.deleteFolder(id);
    folders = folders.filter((f) => f.id !== id);
    // mirror the backend FK (ON DELETE SET NULL): its docs become unfiled
    documents = documents.map((d) => (d.folderId === id ? { ...d, folderId: null } : d));
  }

  // ----- theme -----

  function applyTheme(next: Theme) {
    theme = next;
    document.documentElement.setAttribute("data-theme", next);
    localStorage.setItem(THEME_KEY, next);
  }

  function toggleFocusMode() {
    focusMode = !focusMode;
    localStorage.setItem(FOCUS_KEY, focusMode ? "1" : "0");
    // ensure the preview is fresh when the pane reappears
    if (!focusMode) schedulePreview(content);
  }

  // ----- boot -----

  onMount(() => {
    focusMode = localStorage.getItem(FOCUS_KEY) === "1";
    const stored = localStorage.getItem(THEME_KEY);
    const resolved =
      stored === "light" || stored === "dark"
        ? stored
        : window.matchMedia?.("(prefers-color-scheme: light)").matches
          ? "light"
          : "dark";
    applyTheme(resolved);
    void assistant.init();
    void inlineEdit.init();
    void ideaExpand.init();
    void multiply.init();

    (async () => {
      try {
        [documents, folders, exportTargets] = await Promise.all([
          api.listDocuments(),
          api.listFolders(),
          api.listExportTargets(),
        ]);
        // no auto-select: boot lands on the home shelf
      } catch (e) {
        toast.error(`Loading documents failed: ${formatError(e)}`);
      }
      loading = false;
    })();

    // beforeunload is a backstop (webview reloads); it can't await, so the real
    // quit-safety is the Tauri close hook below.
    const flush = () => void flushSave();
    window.addEventListener("beforeunload", flush);

    // Cmd+Q / native close don't reliably fire beforeunload and tear the webview
    // down before an async save resolves — intercept the close, await the flush,
    // then destroy the window (destroy skips the close-requested cycle).
    let closeUnlisten: (() => void) | null = null;
    let closing = false;
    const appWindow = getCurrentWindow();
    void appWindow
      .onCloseRequested(async (event) => {
        if (closing) return;
        closing = true;
        event.preventDefault();
        try {
          await flushSave();
        } finally {
          await appWindow.destroy();
        }
      })
      .then((un) => (closeUnlisten = un));

    return () => {
      window.removeEventListener("beforeunload", flush);
      closeUnlisten?.();
      void flushSave();
      if (previewTimer) clearTimeout(previewTimer);
      if (exportStatusTimer) clearTimeout(exportStatusTimer);
      assistant.destroy();
      inlineEdit.destroy();
      ideaExpand.destroy();
      multiply.destroy();
    };
  });
</script>

<div class="app-layout {sidebarCollapsed || isHome ? 'app-layout--collapsed' : ''}">
  <Sidebar
    {documents}
    {folders}
    {selectedDocId}
    {expandingId}
    {expandingLabel}
    onSelect={(id) => run(selectDocument(id), "Opening document")}
    onGoHome={() => run(goHome(), "Home")}
    onNewDocument={() => openNewDocument()}
    onNewIdea={newIdea}
    onOpenIdea={(id) => run(openIdea(id), "Opening idea")}
    onExpandIdea={(id, type, label) => run(expandIdea(id, type, label), "Expand idea")}
    onCancelExpand={cancelExpand}
    onConvertIdea={(id, type) => run(convertIdea(id, type), "Convert idea")}
    onRename={(id, name) => run(renameDocument(id, name), "Rename")}
    onDelete={(id) => run(deleteDocument(id), "Delete")}
    onMoveDocument={(id, folderId) => run(moveDocument(id, folderId), "Move")}
    onReorderDocuments={(ids) => run(reorderDocuments(ids), "Reorder")}
    onReorderFolders={(ids) => run(reorderFolders(ids), "Reorder")}
    onCreateFolder={createFolder}
    onRenameFolder={(id, name) => run(renameFolder(id, name), "Rename folder")}
    onDeleteFolder={(id) => run(deleteFolder(id), "Delete folder")}
  />
  <NewDocumentDialog
    open={dialogOpen}
    initialType={dialogInitialType}
    onClose={() => (dialogOpen = false)}
    onCreate={(name, type) => run(createDocumentFromDialog(name, type), "Create document")}
  />
  <IdeaCaptureModal
    open={ideaModalOpen}
    mode={ideaModalMode}
    initialTitle={ideaModalTitle}
    initialBody={ideaModalBody}
    onSave={(title, body) => saveIdea(title, body)}
    onClose={() => (ideaModalOpen = false)}
  />
  <SettingsDialog open={settingsOpen} onClose={() => (settingsOpen = false)} />
  <MultiplyModal
    open={multiplyOpen}
    sourceName={selectedDoc?.name ?? ""}
    isConfigured={assistant.isConfigured}
    progress={multiplyProgress}
    onMultiply={(targets) => run(multiplyDocument(targets), "Multiply")}
    onCancel={cancelMultiply}
    onClose={closeMultiply}
    onOpenSettings={() => (settingsOpen = true)}
  />
  <Toasts />
  <div class="main-content">
    {#if loading}
      <div class="loading"><div class="loading-spinner"></div></div>
    {:else if selectedDoc}
      <TopBar
        documentName={selectedDoc.name}
        {theme}
        onToggleTheme={() => applyTheme(theme === "dark" ? "light" : "dark")}
        {sidebarCollapsed}
        onToggleSidebar={() => (sidebarCollapsed = !sidebarCollapsed)}
        {focusMode}
        onToggleFocus={toggleFocusMode}
        onRename={(name) => run(renameDocument(selectedDoc.id, name), "Rename")}
        {exportTargets}
        onExport={(targetId) => run(exportTo(targetId), "Export")}
        {exportStatus}
        onMultiply={openMultiply}
        onOpenSettings={() => (settingsOpen = true)}
      />
      <Toolbar {editorView} />
      <div class="editor-container {focusMode ? 'editor-container--focus' : ''}">
        <div class="editor-pane">
          {#if docLoading}
            <div class="pane-loading"><div class="loading-spinner"></div></div>
          {/if}
          {#key selectedDoc.id}
            <Editor
              {content}
              {theme}
              {onContentChange}
              onEditorReady={(view) => (editorView = view)}
              onCursorChange={(pos) => (cursorPos = pos)}
            />
          {/key}
        </div>
        {#if !focusMode}
        <div class="preview-pane">
          <RightPaneTabs activeTab={rightTab} onTabChange={changeRightTab} />
          {#if rightTab === "preview"}
            <div id="right-panel-preview" role="tabpanel" aria-labelledby="right-tab-preview" class="preview-panel-body">
            <div class="preview-mode-row">
              <label class="sr-only" for="preview-mode-select">Preview mode</label>
              <select
                id="preview-mode-select"
                class="preview-mode-select"
                value={previewMode}
                onchange={(e) => setPreviewMode(e.currentTarget.value as PreviewMode)}
              >
                <option value="rendered">Rendered</option>
                <option value="linkedin">LinkedIn</option>
                <option value="x-thread">X thread</option>
                <option value="x-article">X Article</option>
              </select>
            </div>
            {#if previewMode === "linkedin"}
              <pre class="linkedin-preview">{linkedinText}</pre>
            {:else if previewMode === "x-thread"}
              <pre class="linkedin-preview">{xThreadText}</pre>
            {:else if previewMode === "x-article"}
              <Preview htmlContent={xArticleHtml} />
            {:else}
              <Preview htmlContent={previewHtml} />
            {/if}
            </div>
          {:else if rightTab === "history"}
            <div id="right-panel-history" role="tabpanel" aria-labelledby="right-tab-history" class="preview-panel-body">
            <HistoryPanel
              {snapshots}
              onSaveSnapshot={() => run(saveSnapshot(), "Save snapshot")}
              onRestore={(id) => run(restoreSnapshot(id), "Restore")}
              getSnapshotContent={(id) => api.getSnapshotContent(id)}
            />
            </div>
          {:else}
            <div id="right-panel-assistant" role="tabpanel" aria-labelledby="right-tab-assistant" class="preview-panel-body">
            <AssistantPanel
              onApply={applyAssistantContent}
              onInsert={insertAssistantContent}
              getDocumentContent={() => content}
              {documents}
              onOpenSettings={() => (settingsOpen = true)}
            />
            </div>
          {/if}
        </div>
        {/if}
      </div>
      <StatusBar documentName={selectedDoc.name} cursorPosition={cursorPos} {wordCount} {saveStatus} />
    {:else}
      <HomeShelf
        {documents}
        {folders}
        onOpenDocument={(id) => run(selectDocument(id), "Opening document")}
        onOpenIdea={(id) => run(openIdea(id), "Opening idea")}
        onCreateProject={createFolder}
        onNewPage={(folderId) => openNewDocument("generic", folderId)}
        onNewPlan={() => openNewDocument("plan")}
        onNewIdea={newIdea}
        onToggleActive={(id, active) => run(toggleFolderActive(id, active), "Update project")}
        isConfigured={assistant.isConfigured}
        {theme}
        onToggleTheme={() => applyTheme(theme === "dark" ? "light" : "dark")}
        onOpenSettings={() => (settingsOpen = true)}
      />
    {/if}
  </div>
</div>
