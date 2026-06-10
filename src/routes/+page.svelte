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
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import { assistant } from "$lib/assistant.svelte";
  import { toast } from "$lib/toast.svelte";

  const SAVE_DEBOUNCE_MS = 500;
  const PREVIEW_DEBOUNCE_MS = 150;
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
  type PreviewMode = "rendered" | "linkedin" | "x-thread";
  let previewMode = $state<PreviewMode>("rendered");
  let linkedinText = $state("");
  let xThreadText = $state("");

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
    }
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
  }

  async function createDocument(name: string, type: DocType) {
    const doc = await api.createDocument(name, type, getTemplate(type) || undefined);
    documents = [doc, ...documents];
    await selectDocument(doc.id);
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
    };
  });
</script>

<div class="app-layout {sidebarCollapsed ? 'app-layout--collapsed' : ''}">
  <Sidebar
    {documents}
    {folders}
    {selectedDocId}
    onSelect={(id) => run(selectDocument(id), "Opening document")}
    onNewDocument={() => (dialogOpen = true)}
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
          <RightPaneTabs activeTab={rightTab} onTabChange={(tab) => (rightTab = tab)} />
          {#if rightTab === "preview"}
            <div class="preview-mode-row">
              <button
                class="preview-mode-btn {previewMode === 'rendered' ? 'preview-mode-btn--active' : ''}"
                onclick={() => setPreviewMode("rendered")}
              >
                Rendered
              </button>
              <button
                class="preview-mode-btn {previewMode === 'linkedin' ? 'preview-mode-btn--active' : ''}"
                onclick={() => setPreviewMode("linkedin")}
              >
                LinkedIn
              </button>
              <button
                class="preview-mode-btn {previewMode === 'x-thread' ? 'preview-mode-btn--active' : ''}"
                onclick={() => setPreviewMode("x-thread")}
              >
                X thread
              </button>
            </div>
            {#if previewMode === "linkedin"}
              <pre class="linkedin-preview">{linkedinText}</pre>
            {:else if previewMode === "x-thread"}
              <pre class="linkedin-preview">{xThreadText}</pre>
            {:else}
              <Preview htmlContent={previewHtml} />
            {/if}
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
