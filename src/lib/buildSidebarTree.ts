import type { Document, Folder } from "$lib/api";

export interface SidebarFolder extends Folder {
  documents: Document[];
}

/** Manual order within a section: ascending `sortOrder`. A stable sort means
    rows that share a (backfilled) sortOrder keep their incoming relative order,
    which is the recency order from `list_documents`. `folders` arrive already
    sorted by the backend, so only the document sections need sorting here. */
const bySortOrder = (a: Document, b: Document) => a.sortOrder - b.sortOrder;

export function buildSidebarTree(
  folders: Folder[],
  documents: Document[],
): {
  folderTree: SidebarFolder[];
  unfiled: Document[];
  ideas: Document[];
  sources: Document[];
} {
  // ideas live in the pinned Inbox section; sources in their own Sources
  // section — neither belongs in the editable folder tree.
  const ideas = documents.filter((d) => d.type === "idea").sort(bySortOrder);
  const sources = documents.filter((d) => d.type === "source").sort(bySortOrder);
  const docs = documents.filter((d) => d.type !== "idea" && d.type !== "source");

  const folderTree: SidebarFolder[] = folders.map((f) => ({
    ...f,
    documents: docs.filter((d) => d.folderId === f.id).sort(bySortOrder),
  }));

  const unfiled = docs.filter((d) => !d.folderId).sort(bySortOrder);

  return { folderTree, unfiled, ideas, sources };
}
