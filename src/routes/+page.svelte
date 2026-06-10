<script lang="ts">
  import { onMount } from "svelte";
  import type { EditorView } from "@codemirror/view";
  import { api, type DocType, type Document, type Folder } from "$lib/api";
  import { getTemplate } from "$lib/templates";
  import type { Theme } from "$lib/editor/themes";
  import Editor from "$lib/components/Editor.svelte";
  import Toolbar from "$lib/components/Toolbar.svelte";
  import TopBar from "$lib/components/TopBar.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import Preview from "$lib/components/Preview.svelte";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import NewDocumentDialog from "$lib/components/NewDocumentDialog.svelte";
  import RightPaneTabs, { type RightPaneTab } from "$lib/components/RightPaneTabs.svelte";
  import AssistantPanel from "$lib/components/AssistantPanel.svelte";
  import HistoryPanel from "$lib/components/HistoryPanel.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import { assistant } from "$lib/assistant.svelte";
  import { inlineEdit } from "$lib/inlineEdit.svelte";
  import { ideaExpand } from "$lib/ideaExpand.svelte";
  import { toast } from "$lib/toast.svelte";
  import type { SnapshotMeta } from "$lib/api";

  const SAVE_DEBOUNCE_MS = 500;
  const PREVIEW_DEBOUNCE_MS = 150;
  /** How often active editing produces an automatic version snapshot. */
  const SNAPSHOT_INTERVAL_MS = 10 * 60 * 1000;
  const THEME_KEY = "markdown-theme";

  let documents = $state<Document[]>([]);
  let folders = $state<Folder[]>([]);
  let selectedDocId = $state<string | null>(null);
  /** Live text of the selected doc. Seeds the editor on remount ({#key});
      after that the editor owns the text and reports changes back here. */
  let content = $state("");
  let loading = $state(true);

  let theme = $state<Theme>("dark");
  let sidebarCollapsed = $state(false);
  let dialogOpen = $state(false);
  let settingsOpen = $state(false);
  type PreviewMode = "rendered" | "linkedin" | "x-thread" | "x-article";
  let previewMode = $state<PreviewMode>("rendered");
  let linkedinText = $state("");
  let xThreadText = $state("");
  let xArticleHtml = $state("");

  /** Fire-and-forget with a visible error toast on failure. */
  function run(promise: Promise<unknown>, what: string) {
    promise.catch((e) => toast.error(`${what} failed: ${e}`));
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

  // ----- persistence (debounced save) -----

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  /** Unsaved content per document. Failed saves stay here until they land,
      so switching documents (and typing there) can't clobber them. */
  const pendingSaves = new Map<string, string>();

  function scheduleSave(docId: string, content: string) {
    pendingSaves.set(docId, content);
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(flushSave, SAVE_DEBOUNCE_MS);
  }

  async function flushSave() {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    for (const [id, content] of [...pendingSaves]) {
      pendingSaves.delete(id);
      try {
        await api.saveDocumentContent(id, content);
      } catch (e) {
        // keep it for the next keystroke/flush to retry — unless newer
        // content for this doc was scheduled while the save was in flight
        if (!pendingSaves.has(id)) pendingSaves.set(id, content);
        toast.error(`Saving failed: ${e}`);
        continue;
      }
      const doc = documents.find((d) => d.id === id);
      if (doc) doc.updatedAt = new Date().toISOString();
      // an idea is titled by its first line (notes-app style) — keep in sync
      if (doc && doc.type === "idea") {
        const name = deriveIdeaName(content);
        if (name !== doc.name) run(renameDocument(id, name), "Rename idea");
      }
      maybeIntervalSnapshot(id, content);
    }
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
      .catch((e) => toast.error(`Snapshot failed: ${e}`));
  }

  // ----- preview (debounced render via comrak) -----

  let previewTimer: ReturnType<typeof setTimeout> | null = null;

  function schedulePreview(content: string) {
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

  function applyAssistantContent(content: string) {
    if (!editorView) return;
    // dispatch fires the editor's updateListener, so save + preview follow
    editorView.dispatch({
      changes: { from: 0, to: editorView.state.doc.length, insert: content },
    });
  }

  async function exportTo(targetId: string) {
    if (!selectedDoc) return;
    await flushSave();
    try {
      const result = await api.exportDocument(content, selectedDoc.name, targetId);
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
      }
    } catch (e) {
      showExportStatus(`Export failed: ${e}`);
    }
  }

  function showExportStatus(message: string) {
    exportStatus = message;
    if (exportStatusTimer) clearTimeout(exportStatusTimer);
    exportStatusTimer = setTimeout(() => (exportStatus = ""), 5000);
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
    // safety capture of the current text, then swap in the restored version
    await api.createSnapshot(selectedDocId, content, "restore");
    const restored = await api.getSnapshotContent(snapshotId);
    // dispatch so the editor's updateListener drives save + preview (one-way flow)
    editorView.dispatch({
      changes: { from: 0, to: editorView.state.doc.length, insert: restored },
    });
    await loadSnapshots();
  }

  function changeRightTab(tab: RightPaneTab) {
    rightTab = tab;
    if (tab === "history") run(loadSnapshots(), "Loading history");
  }

  // ----- document/folder actions -----

  async function selectDocument(id: string) {
    if (id === selectedDocId) return;
    await flushSave();
    editorView = null;
    content = await api.getDocumentContent(id);
    selectedDocId = id;
    schedulePreview(content);
    wordCount = countWords(content);
    void assistant.loadFor(id);
    inlineEdit.setContext(id, () => content, refreshHistoryIfOpen);
    if (rightTab === "history") void loadSnapshots();
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
  }

  // ----- idea inbox -----

  /** Id of the idea currently being expanded — drives the sidebar spinner. */
  let expandingId = $state<string | null>(null);

  /** An idea's title is its first non-empty line (markdown heading marks
      stripped); empty → "New idea". */
  function deriveIdeaName(content: string): string {
    const firstLine = content.split("\n").find((l) => l.trim()) ?? "";
    const cleaned = firstLine.replace(/^#+\s*/, "").trim();
    if (!cleaned) return "New idea";
    return cleaned.length > 40 ? `${cleaned.slice(0, 40)}…` : cleaned;
  }

  /** "+ New idea": create a blank idea and open it for immediate capture. */
  function newIdea() {
    run(createDocument("New idea", "idea"), "New idea");
  }

  /** Expand an idea into a full draft of `type`, then open the draft. The idea
      itself is kept in the Inbox. */
  async function expandIdea(ideaId: string, type: DocType, label: string) {
    const idea = documents.find((d) => d.id === ideaId);
    // flush so a just-typed idea (still in the debounce window) is read in full
    await flushSave();
    const ideaText = await api.getDocumentContent(ideaId);
    if (!ideaText.trim()) {
      toast.error("Add some text to the idea before expanding it.");
      return;
    }
    expandingId = ideaId;
    try {
      const draft = await ideaExpand.expand(ideaText, label);
      const name = idea && idea.name !== "New idea" ? `${idea.name} — ${label}` : `${label} draft`;
      await createDocument(name, type, draft);
    } finally {
      expandingId = null;
    }
  }

  async function renameDocument(id: string, name: string) {
    const updated = await api.renameDocument(id, name);
    documents = documents.map((d) => (d.id === id ? updated : d));
  }

  async function moveDocument(id: string, folderId: string | null) {
    const updated = await api.moveDocument(id, folderId);
    documents = documents.map((d) => (d.id === id ? updated : d));
  }

  async function deleteDocument(id: string) {
    await api.deleteDocument(id);
    pendingSaves.delete(id); // never retry a save against a deleted row
    documents = documents.filter((d) => d.id !== id);
    if (selectedDocId === id) {
      selectedDocId = null;
      if (documents.length > 0) {
        await selectDocument(documents[0].id);
      } else {
        void assistant.loadFor(null);
      }
    }
  }

  async function createFolder(name: string): Promise<Folder> {
    const folder = await api.createFolder(name);
    folders = [...folders, folder];
    return folder;
  }

  async function renameFolder(id: string, name: string) {
    const updated = await api.renameFolder(id, name);
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

  // ----- boot -----

  onMount(() => {
    const stored = localStorage.getItem(THEME_KEY);
    applyTheme(stored === "light" || stored === "dark" ? stored : "dark");
    void assistant.init();
    void inlineEdit.init();
    void ideaExpand.init();

    (async () => {
      try {
        [documents, folders, exportTargets] = await Promise.all([
          api.listDocuments(),
          api.listFolders(),
          api.listExportTargets(),
        ]);
        if (documents.length > 0) {
          await selectDocument(documents[0].id);
        }
      } catch (e) {
        toast.error(`Loading documents failed: ${e}`);
      }
      loading = false;
    })();

    const flush = () => void flushSave();
    window.addEventListener("beforeunload", flush);
    return () => {
      window.removeEventListener("beforeunload", flush);
      void flushSave();
      assistant.destroy();
      inlineEdit.destroy();
      ideaExpand.destroy();
    };
  });
</script>

<div class="app-layout {sidebarCollapsed ? 'app-layout--collapsed' : ''}">
  <Sidebar
    {documents}
    {folders}
    {selectedDocId}
    {expandingId}
    onSelect={(id) => run(selectDocument(id), "Opening document")}
    onNewDocument={() => (dialogOpen = true)}
    onNewIdea={newIdea}
    onExpandIdea={(id, type, label) => run(expandIdea(id, type, label), "Expand idea")}
    onRename={(id, name) => run(renameDocument(id, name), "Rename")}
    onDelete={(id) => run(deleteDocument(id), "Delete")}
    onMoveDocument={(id, folderId) => run(moveDocument(id, folderId), "Move")}
    onCreateFolder={createFolder}
    onRenameFolder={(id, name) => run(renameFolder(id, name), "Rename folder")}
    onDeleteFolder={(id) => run(deleteFolder(id), "Delete folder")}
  />
  <NewDocumentDialog
    open={dialogOpen}
    onClose={() => (dialogOpen = false)}
    onCreate={(name, type) => run(createDocument(name, type), "Create document")}
  />
  <SettingsDialog open={settingsOpen} onClose={() => (settingsOpen = false)} />
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
        onRename={(name) => run(renameDocument(selectedDoc.id, name), "Rename")}
        {exportTargets}
        onExport={(targetId) => run(exportTo(targetId), "Export")}
        {exportStatus}
        onOpenSettings={() => (settingsOpen = true)}
      />
      <Toolbar {editorView} />
      <div class="editor-container">
        <div class="editor-pane">
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
        <div class="preview-pane">
          <RightPaneTabs activeTab={rightTab} onTabChange={changeRightTab} />
          {#if rightTab === "preview"}
            <div class="preview-mode-row">
              <select
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
          {:else if rightTab === "history"}
            <HistoryPanel
              {snapshots}
              onSaveSnapshot={() => run(saveSnapshot(), "Save snapshot")}
              onRestore={(id) => run(restoreSnapshot(id), "Restore")}
              getSnapshotContent={(id) => api.getSnapshotContent(id)}
            />
          {:else}
            <AssistantPanel
              onApply={applyAssistantContent}
              onInsert={insertAssistantContent}
              getDocumentContent={() => content}
              onOpenSettings={() => (settingsOpen = true)}
            />
          {/if}
        </div>
      </div>
      <StatusBar documentName={selectedDoc.name} cursorPosition={cursorPos} {wordCount} />
    {:else}
      <div class="empty-state">
        <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" opacity="0.15">
          <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
          <polyline points="14,2 14,8 20,8" />
          <line x1="16" y1="13" x2="8" y2="13" />
          <line x1="16" y1="17" x2="8" y2="17" />
        </svg>
        <h2>No document selected</h2>
        <p>Create a document or select one from the sidebar</p>
      </div>
    {/if}
  </div>
</div>
