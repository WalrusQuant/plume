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
  let initialContent = $state("");
  let loading = $state(true);

  let theme = $state<Theme>("dark");
  let sidebarCollapsed = $state(false);
  let dialogOpen = $state(false);
  let settingsOpen = $state(false);
  let previewMode = $state<"rendered" | "linkedin">("rendered");
  let linkedinText = $state("");

  /** Fire-and-forget with a visible error toast on failure. */
  function run(promise: Promise<unknown>, what: string) {
    promise.catch((e) => toast.error(`${what} failed: ${e}`));
  }

  let editorView = $state<EditorView | null>(null);
  let cursorPos = $state({ line: 1, col: 1 });
  let previewHtml = $state("");
  let wordCount = $state(0);
  let rightTab = $state<RightPaneTab>("preview");
  let currentContent = "";
  let exportTargets = $state<import("$lib/api").ExportTarget[]>([]);
  let exportStatus = $state("");
  let exportStatusTimer: ReturnType<typeof setTimeout> | null = null;

  const selectedDoc = $derived(documents.find((d) => d.id === selectedDocId));

  // ----- persistence (debounced save) -----

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let pendingContent: string | null = null;
  let savingDocId: string | null = null;

  function scheduleSave(docId: string, content: string) {
    savingDocId = docId;
    pendingContent = content;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(flushSave, SAVE_DEBOUNCE_MS);
  }

  async function flushSave() {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (savingDocId && pendingContent !== null) {
      const id = savingDocId;
      const content = pendingContent;
      pendingContent = null;
      try {
        await api.saveDocumentContent(id, content);
      } catch (e) {
        pendingContent = content; // keep it; the next keystroke or flush retries
        toast.error(`Saving failed: ${e}`);
        return;
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
    } else {
      previewHtml = await api.renderPreview(content);
    }
  }

  function setPreviewMode(mode: "rendered" | "linkedin") {
    previewMode = mode;
    run(updatePreview(currentContent), "Preview");
  }

  function countWords(content: string): number {
    const words = content.trim().match(/\S+/g);
    return words ? words.length : 0;
  }

  function onContentChange(content: string) {
    currentContent = content;
    if (selectedDocId) scheduleSave(selectedDocId, content);
    schedulePreview(content);
    wordCount = countWords(content);
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
      const result = await api.exportDocument(currentContent, selectedDoc.name, targetId);
      if (result.type === "clipboard") {
        await navigator.clipboard.writeText(result.text);
        showExportStatus("Copied to clipboard — ready to paste");
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
    initialContent = await api.getDocumentContent(id);
    selectedDocId = id;
    currentContent = initialContent;
    schedulePreview(initialContent);
    wordCount = countWords(initialContent);
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
    documents = documents.filter((d) => d.id !== id);
    if (selectedDocId === id) {
      // discard any pending save for the deleted doc
      pendingContent = null;
      savingDocId = null;
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
              content={initialContent}
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
            </div>
            {#if previewMode === "linkedin"}
              <pre class="linkedin-preview">{linkedinText}</pre>
            {:else}
              <Preview htmlContent={previewHtml} />
            {/if}
          {:else}
            <AssistantPanel
              onApply={applyAssistantContent}
              onInsert={insertAssistantContent}
              getDocumentContent={() => currentContent}
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
