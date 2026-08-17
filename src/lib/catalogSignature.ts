import type { Document, Folder } from "$lib/api";

/** Compact fingerprint of the sidebar/shelf catalog. Used to skip applying a
    refresh when the notebook hasn't changed since the last fetch. */
export function catalogSignature(documents: Document[], folders: Folder[]): string {
  const docs = documents
    .map(
      (d) =>
        `${d.id}\t${d.name}\t${d.type}\t${d.folderId ?? ""}\t${d.updatedAt}\t${d.sortOrder}\t${d.titleExplicit ? 1 : 0}`,
    )
    .join("\n");
  const folds = folders
    .map(
      (f) =>
        `${f.id}\t${f.name}\t${f.parentId ?? ""}\t${f.updatedAt}\t${f.active ? 1 : 0}\t${f.sortOrder}`,
    )
    .join("\n");
  return `${docs}\n--\n${folds}`;
}
