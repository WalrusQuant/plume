import type { DocType } from "$lib/api";

/** The platform target types a document can be multiplied into (and that an idea
    can be expanded into). The label is passed to the AI adaptation prompt and
    shown in the picker. Each maps to an export renderer. */
export interface MultiplyTarget {
  type: DocType;
  label: string;
}

export const MULTIPLY_TARGETS: MultiplyTarget[] = [
  { type: "blog-post", label: "Blog Post" },
  { type: "newsletter", label: "Newsletter" },
  { type: "linkedin-post", label: "LinkedIn Post" },
  { type: "x-thread", label: "X Thread" },
];

export type MultiplyStatus = "pending" | "running" | "done" | "error";

/** One row of multiply progress — a chosen target and its generation state. */
export interface MultiplyProgress extends MultiplyTarget {
  status: MultiplyStatus;
}
