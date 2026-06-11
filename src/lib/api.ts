import { invoke } from "@tauri-apps/api/core";

export type DocType =
  | "blog-post"
  | "newsletter"
  | "linkedin-post"
  | "x-thread"
  | "skill"
  | "claude-md"
  | "system-prompt"
  | "runbook"
  | "idea"
  | "generic";

export interface Document {
  id: string;
  name: string;
  type: DocType;
  folderId: string | null;
  createdAt: string;
  updatedAt: string;
  /** True once the name was set deliberately; false means it's derived (ideas:
      from the first line) and may be auto-updated. */
  titleExplicit: boolean;
}

export interface Folder {
  id: string;
  name: string;
  parentId: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  /** Token usage, present on assistant messages once a stream completes. */
  inputTokens?: number;
  outputTokens?: number;
}

export interface Chat {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
}

export type AIProvider = "anthropic" | "openrouter";

export type SnapshotCause = "ai-edit" | "interval" | "manual" | "restore";

export interface SnapshotMeta {
  id: string;
  cause: SnapshotCause;
  wordCount: number;
  createdAt: string;
}

export interface ExportTarget {
  id: string;
  label: string;
  delivery: "clipboard" | "file";
  ext: string | null;
}

export type ExportOutput =
  | { type: "clipboard"; text: string }
  | { type: "clipboardHtml"; html: string; plain: string }
  | { type: "file"; path: string }
  | { type: "cancelled" };

export const api = {
  listDocuments: () => invoke<Document[]>("list_documents"),
  createDocument: (name: string, docType?: DocType, content?: string) =>
    invoke<Document>("create_document", { name, docType, content }),
  renameDocument: (id: string, name: string) =>
    invoke<Document>("rename_document", { id, name }),
  updateIdeaName: (id: string, name: string, explicit: boolean) =>
    invoke<Document>("update_idea_name", { id, name, explicit }),
  updateDocumentType: (
    id: string,
    docType: DocType,
    name: string,
    explicit: boolean,
  ) => invoke<Document>("update_document_type", { id, docType, name, explicit }),
  moveDocument: (id: string, folderId: string | null) =>
    invoke<Document>("move_document", { id, folderId }),
  deleteDocument: (id: string) => invoke<void>("delete_document", { id }),
  getDocumentContent: (id: string) =>
    invoke<string>("get_document_content", { id }),
  saveDocumentContent: (id: string, content: string) =>
    invoke<void>("save_document_content", { id, content }),

  renderPreview: (content: string) => invoke<string>("render_preview", { content }),
  renderLinkedinPreview: (content: string) =>
    invoke<string>("render_linkedin_preview", { content }),
  renderXThreadPreview: (content: string) =>
    invoke<string>("render_x_thread_preview", { content }),
  renderXArticlePreview: (content: string) =>
    invoke<string>("render_x_article_preview", { content }),

  listFolders: () => invoke<Folder[]>("list_folders"),
  createFolder: (name: string) => invoke<Folder>("create_folder", { name }),
  renameFolder: (id: string, name: string) =>
    invoke<Folder>("rename_folder", { id, name }),
  deleteFolder: (id: string) => invoke<void>("delete_folder", { id }),

  listExportTargets: () => invoke<ExportTarget[]>("list_export_targets"),
  exportDocument: (content: string, docName: string, targetId: string) =>
    invoke<ExportOutput>("export_document", { content, docName, targetId }),

  listChats: (documentId: string) => invoke<Chat[]>("list_chats", { documentId }),
  createChat: (documentId: string, title?: string) =>
    invoke<Chat>("create_chat", { documentId, title }),
  renameChat: (chatId: string, title: string) =>
    invoke<Chat>("rename_chat", { chatId, title }),
  deleteChat: (chatId: string) => invoke<void>("delete_chat", { chatId }),
  getChatMessages: (chatId: string) =>
    invoke<ChatMessage[]>("get_chat_messages", { chatId }),
  saveChatMessages: (chatId: string, messages: ChatMessage[]) =>
    invoke<void>("save_chat_messages", { chatId, messages }),

  createSnapshot: (documentId: string, content: string, cause: SnapshotCause) =>
    invoke<SnapshotMeta | null>("create_snapshot", { documentId, content, cause }),
  listSnapshots: (documentId: string) =>
    invoke<SnapshotMeta[]>("list_snapshots", { documentId }),
  getSnapshotContent: (snapshotId: string) =>
    invoke<string>("get_snapshot_content", { snapshotId }),

  setApiKey: (provider: AIProvider, key: string) =>
    invoke<void>("set_api_key", { provider, key }),
  hasApiKey: (provider: AIProvider) => invoke<boolean>("has_api_key", { provider }),
  deleteApiKey: (provider: AIProvider) => invoke<void>("delete_api_key", { provider }),
  sendAssistantMessage: (
    streamId: string,
    provider: AIProvider,
    model: string | null,
    messages: ChatMessage[],
    documentContent: string,
    voice: string | null,
  ) =>
    invoke<void>("send_assistant_message", {
      streamId,
      provider,
      model,
      messages,
      documentContent,
      voice,
    }),
  sendInlineEdit: (
    streamId: string,
    provider: AIProvider,
    model: string | null,
    instruction: string,
    selectedText: string,
    documentContent: string,
    voice: string | null,
  ) =>
    invoke<void>("send_inline_edit", {
      streamId,
      provider,
      model,
      instruction,
      selectedText,
      documentContent,
      voice,
    }),
  sendIdeaExpand: (
    streamId: string,
    provider: AIProvider,
    model: string | null,
    idea: string,
    targetLabel: string,
    voice: string | null,
  ) =>
    invoke<void>("send_idea_expand", { streamId, provider, model, idea, targetLabel, voice }),
  stopAssistant: () => invoke<void>("stop_assistant"),
};
