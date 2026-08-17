import { describe, it, expect } from "vitest";
import { catalogSignature } from "$lib/catalogSignature";
import type { Document, Folder } from "$lib/api";

function makeDoc(overrides: Partial<Document> = {}): Document {
  return {
    id: "d1",
    name: "Plan",
    type: "plan",
    folderId: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    titleExplicit: true,
    sortOrder: 0,
    ...overrides,
  };
}

function makeFolder(overrides: Partial<Folder> = {}): Folder {
  return {
    id: "f1",
    name: "Project",
    parentId: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    active: true,
    sortOrder: 0,
    ...overrides,
  };
}

describe("catalogSignature", () => {
  it("matches identical catalogs", () => {
    const docs = [makeDoc()];
    const folders = [makeFolder()];
    expect(catalogSignature(docs, folders)).toBe(catalogSignature(docs, folders));
  });

  it("changes when a document is added", () => {
    const before = catalogSignature([makeDoc()], []);
    const after = catalogSignature([makeDoc(), makeDoc({ id: "d2", name: "New" })], []);
    expect(after).not.toBe(before);
  });

  it("changes when a document is filed in a project", () => {
    const before = catalogSignature([makeDoc()], [makeFolder()]);
    const after = catalogSignature([makeDoc({ folderId: "f1" })], [makeFolder()]);
    expect(after).not.toBe(before);
  });

  it("changes when a folder is added", () => {
    const before = catalogSignature([], []);
    const after = catalogSignature([], [makeFolder()]);
    expect(after).not.toBe(before);
  });

  it("changes when a document is renamed or reordered", () => {
    const base = catalogSignature([makeDoc()], []);
    expect(catalogSignature([makeDoc({ name: "Renamed" })], [])).not.toBe(base);
    expect(catalogSignature([makeDoc({ sortOrder: 3 })], [])).not.toBe(base);
  });
});
