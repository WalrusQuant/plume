import { describe, it, expect } from "vitest";
import { buildSidebarTree } from "$lib/buildSidebarTree";
import type { Document, Folder } from "$lib/api";

function makeDoc(
  id: string,
  type: Document["type"],
  sortOrder: number,
  folderId: string | null = null,
): Document {
  return {
    id,
    name: `Doc ${id}`,
    type,
    folderId,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    titleExplicit: true,
    sortOrder,
  };
}

function makeFolder(id: string, sortOrder: number): Folder {
  return {
    id,
    name: `Folder ${id}`,
    parentId: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    active: true,
    sortOrder,
  };
}

describe("buildSidebarTree", () => {
  it("separates ideas from regular docs", () => {
    const docs = [
      makeDoc("d1", "generic", 0),
      makeDoc("i1", "idea", 0),
      makeDoc("d2", "blog-post", 1),
      makeDoc("i2", "idea", 1),
    ];
    const { ideas, unfiled, folderTree } = buildSidebarTree([], docs);
    expect(ideas).toHaveLength(2);
    expect(ideas.map((d) => d.id)).toEqual(["i1", "i2"]);
    expect(unfiled).toHaveLength(2);
    expect(folderTree).toHaveLength(0);
  });

  it("places docs in their folder", () => {
    const folders = [makeFolder("f1", 0)];
    const docs = [
      makeDoc("d1", "generic", 1, "f1"),
      makeDoc("d2", "generic", 0, "f1"),
      makeDoc("d3", "generic", 0, null),
    ];
    const { folderTree, unfiled } = buildSidebarTree(folders, docs);
    expect(folderTree).toHaveLength(1);
    expect(folderTree[0].documents.map((d) => d.id)).toEqual(["d2", "d1"]); // sorted by sortOrder
    expect(unfiled.map((d) => d.id)).toEqual(["d3"]);
  });

  it("sorts each section by sortOrder ascending", () => {
    const docs = [
      makeDoc("c", "generic", 2),
      makeDoc("a", "generic", 0),
      makeDoc("b", "generic", 1),
    ];
    const { unfiled } = buildSidebarTree([], docs);
    expect(unfiled.map((d) => d.id)).toEqual(["a", "b", "c"]);
  });

  it("keeps incoming order for equal sortOrder (stable sort)", () => {
    const docs = [
      makeDoc("first", "generic", 5),
      makeDoc("second", "generic", 5),
      makeDoc("third", "generic", 5),
    ];
    const { unfiled } = buildSidebarTree([], docs);
    expect(unfiled.map((d) => d.id)).toEqual(["first", "second", "third"]);
  });

  it("handles empty input", () => {
    const { ideas, unfiled, folderTree } = buildSidebarTree([], []);
    expect(ideas).toEqual([]);
    expect(unfiled).toEqual([]);
    expect(folderTree).toEqual([]);
  });

  it("handles docs in a non-existent folder (orphaned)", () => {
    const docs = [makeDoc("d1", "generic", 0, "ghost-folder")];
    const { unfiled, folderTree } = buildSidebarTree([], docs);
    // orphaned docs (folderId set but folder not in list) are NOT unfiled and
    // NOT in any folder — they disappear from the tree. This is the existing
    // behavior (filtering on !folderId for unfiled).
    expect(unfiled).toHaveLength(0);
    expect(folderTree).toHaveLength(0);
  });
});
