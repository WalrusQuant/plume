import type { DocType } from "$lib/api";

export interface DocumentTypeConfig {
  type: DocType;
  label: string;
  description: string;
}

/** Blank first, then creator types (the product pitch), then agent-file bonus tier. */
export const DOCUMENT_TYPES: DocumentTypeConfig[] = [
  {
    type: "generic",
    label: "Blank",
    description: "Empty document — start from scratch",
  },
  {
    type: "blog-post",
    label: "Blog Post",
    description: "Long-form post for your blog or publication",
  },
  {
    type: "newsletter",
    label: "Newsletter",
    description: "Email issue for Substack, beehiiv, or Ghost",
  },
  {
    type: "linkedin-post",
    label: "LinkedIn Post",
    description: "Short professional post with a hook",
  },
  {
    type: "x-thread",
    label: "X Thread",
    description: "Multi-post thread for X/Twitter",
  },
  {
    type: "plan",
    label: "Plan",
    description: "Lightweight build plan — the start of a build-in-public loop",
  },
  {
    type: "build-log",
    label: "Build Log",
    description: "Dated working notes — raw material for posts",
  },
  {
    type: "skill",
    label: "Skill",
    description: "Claude Code skill with YAML frontmatter",
  },
  {
    type: "claude-md",
    label: "CLAUDE.md",
    description: "Project instructions file for Claude Code",
  },
  {
    type: "system-prompt",
    label: "System Prompt",
    description: "System prompt for an AI agent or assistant",
  },
  {
    type: "runbook",
    label: "Runbook",
    description: "Operational runbook with steps and procedures",
  },
];

// Compile-time exhaustiveness guard: every DocType must either appear in
// DOCUMENT_TYPES or be explicitly excluded below. Adding a new DocType variant
// without registering it here is a type error.
const _EXHAUSTIVE: Record<DocType, true> = {
  "blog-post": true,
  newsletter: true,
  "linkedin-post": true,
  "x-thread": true,
  skill: true,
  "claude-md": true,
  "system-prompt": true,
  runbook: true,
  plan: true,
  "build-log": true,
  // `idea` is intentionally excluded from DOCUMENT_TYPES — ideas are captured
  // via the IdeaCaptureModal, never the new-doc picker.
  idea: true,
  generic: true,
};
void _EXHAUSTIVE;
