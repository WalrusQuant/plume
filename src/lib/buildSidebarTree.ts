import type { Document, Folder } from "$lib/api";

export interface SidebarFolder extends Folder {
  documents: Document[];
}

export function buildSidebarTree(
  folders: Folder[],
  documents: Document[],
): { folderTree: SidebarFolder[]; unfiled: Document[]; ideas: Document[] } {
  // ideas live in the pinned Inbox section, never in the folder tree
  const ideas = documents.filter((d) => d.type === "idea");
  const docs = documents.filter((d) => d.type !== "idea");

  const folderTree: SidebarFolder[] = folders.map((f) => ({
    ...f,
    documents: docs.filter((d) => d.folderId === f.id),
  }));

  const unfiled = docs.filter((d) => !d.folderId);

  return { folderTree, unfiled, ideas };
}
