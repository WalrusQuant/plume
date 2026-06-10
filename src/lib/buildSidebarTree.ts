import type { Document, Folder } from "$lib/api";

export interface SidebarFolder extends Folder {
  documents: Document[];
}

export function buildSidebarTree(
  folders: Folder[],
  documents: Document[],
): { folderTree: SidebarFolder[]; unfiled: Document[] } {
  const folderTree: SidebarFolder[] = folders.map((f) => ({
    ...f,
    documents: documents.filter((d) => d.folderId === f.id),
  }));

  const unfiled = documents.filter((d) => !d.folderId);

  return { folderTree, unfiled };
}
