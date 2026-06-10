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

export function typeLabel(type: DocType): string {
  return DOCUMENT_TYPES.find((t) => t.type === type)?.label ?? "Document";
}
