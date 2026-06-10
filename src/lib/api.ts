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
  | "generic";

export interface Document {
  id: string;
  name: string;
  type: DocType;
  folderId: string | null;
  createdAt: string;
  updatedAt: string;
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
}

export type AIProvider = "anthropic" | "openrouter";

export interface ExportTarget {
  id: string;
  label: string;
  delivery: "clipboard" | "file";
  ext: string | null;
}

export type ExportOutput =
  | { type: "clipboard"; text: string }
  | { type: "file"; path: string }
  | { type: "cancelled" };

export const api = {
  listDocuments: () => invoke<Document[]>("list_documents"),
  createDocument: (name: string, docType?: DocType, content?: string) =>
    invoke<Document>("create_document", { name, docType, content }),
  renameDocument: (id: string, name: string) =>
    invoke<Document>("rename_document", { id, name }),
  moveDocument: (id: string, folderId: string | null) =>
    invoke<Document>("move_document", { id, folderId }),
  deleteDocument: (id: string) => invoke<void>("delete_document", { id }),
  getDocumentContent: (id: string) =>
    invoke<string>("get_document_content", { id }),
  saveDocumentContent: (id: string, content: string) =>
    invoke<void>("save_document_content", { id, content }),

  renderPreview: (content: string) => invoke<string>("render_preview", { content }),

  listFolders: () => invoke<Folder[]>("list_folders"),
  createFolder: (name: string) => invoke<Folder>("create_folder", { name }),
  renameFolder: (id: string, name: string) =>
    invoke<Folder>("rename_folder", { id, name }),
  deleteFolder: (id: string) => invoke<void>("delete_folder", { id }),

  listExportTargets: () => invoke<ExportTarget[]>("list_export_targets"),
  exportDocument: (content: string, docName: string, targetId: string) =>
    invoke<ExportOutput>("export_document", { content, docName, targetId }),

  getChatMessages: (documentId: string) =>
    invoke<ChatMessage[]>("get_chat_messages", { documentId }),
  saveChatMessages: (documentId: string, messages: ChatMessage[]) =>
    invoke<void>("save_chat_messages", { documentId, messages }),

  setApiKey: (provider: AIProvider, key: string) =>
    invoke<void>("set_api_key", { provider, key }),
  hasApiKey: (provider: AIProvider) => invoke<boolean>("has_api_key", { provider }),
  deleteApiKey: (provider: AIProvider) => invoke<void>("delete_api_key", { provider }),
  sendAssistantMessage: (
    provider: AIProvider,
    model: string | null,
    messages: ChatMessage[],
    documentContent: string,
  ) => invoke<void>("send_assistant_message", { provider, model, messages, documentContent }),
  stopAssistant: () => invoke<void>("stop_assistant"),
};
