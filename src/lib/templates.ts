import type { DocType } from "$lib/api";

const blogPostTemplate = `# Post Title

> One-sentence hook: why should anyone read this?

## Introduction

Set up the problem or promise. What will the reader walk away with?

## Main Point 1

## Main Point 2

## Main Point 3

## Conclusion

Wrap up and give the reader one clear next step.
`;

const newsletterTemplate = `# Issue Title

**Subject line:** the email subject readers will see
**Preview text:** the snippet after the subject

---

Hey friends,

Open with something personal or timely.

## The Main Story

## Worth Your Time

- Link one — why it matters
- Link two — why it matters

## One More Thing

Sign-off,
Your name
`;

const linkedinPostTemplate = `The hook line — stop the scroll. Make a bold claim or ask a sharp question.

Tell the story in short paragraphs.

One idea per line.

White space is your friend.

End with a takeaway or a question to drive comments.

---
*Posting tip: the first two lines show before the "...see more" fold.*
`;

const xThreadTemplate = `The hook tweet. Big claim, surprising fact, or a promise of value. 🧵

---

Point one. One idea per tweet, keep each under 280 characters.

---

Point two. Short sentences punch harder.

---

The recap + call to action. Follow for more, link to the full post.
`;

const skillTemplate = `---
name: skill-name
description: Short description of what this skill does and when to use it
---

# Skill Name

## Instructions

Describe what this skill should do when triggered.

## Steps

1. First, do this
2. Then, do that
3. Finally, return the result

## Examples

\`\`\`
User: example input
Assistant: example output
\`\`\`
`;

const claudeMdTemplate = `# Project Instructions

## Overview

Describe the project and its purpose.

## Tech Stack

- **Frontend:**
- **Backend:**
- **Database:**

## Coding Standards

-
-
-

## File Structure

\`\`\`
src/
  components/
  hooks/
  utils/
\`\`\`

## Important Notes

-
`;

const systemPromptTemplate = `# System Prompt

You are an AI assistant that...

## Role

Describe the role and persona.

## Capabilities

-
-
-

## Constraints

-
-
-

## Response Format

Describe the expected output format.

## Examples

### Example 1

**User:**
**Assistant:**
`;

const runbookTemplate = `# Runbook: [Title]

## Overview

What this runbook covers and when to use it.

## Prerequisites

- [ ] Access to...
- [ ] Familiarity with...

## Steps

### 1. [First Step]

\`\`\`bash
# commands here
\`\`\`

### 2. [Second Step]

Description of what to do.

### 3. [Verification]

How to verify the operation succeeded.

## Rollback

Steps to undo if something goes wrong.

## Contacts

| Role | Name | Contact |
| --- | --- | --- |
| Owner | | |
| Escalation | | |
`;

const planTemplate = `# Plan: [What you're building]

## Why

The itch you're scratching, and who it's for. One paragraph.

## Shape of done

What exists when this is finished — an outcome, not a task list.

## Steps

Rough order, not a contract.

1.
2.
3.

## Worth writing about

Moments in this build that could become posts.

-
`;

const buildLogTemplate = `# Build log: [Project]

Add a new dated section per session — this file is the raw ore your posts
are mined from.

---

## YYYY-MM-DD

**Did:**

**Hit:**

**Learned:**

**Post-worthy:**
`;

const templates: Record<DocType, string> = {
  "blog-post": blogPostTemplate,
  newsletter: newsletterTemplate,
  "linkedin-post": linkedinPostTemplate,
  "x-thread": xThreadTemplate,
  skill: skillTemplate,
  "claude-md": claudeMdTemplate,
  "system-prompt": systemPromptTemplate,
  runbook: runbookTemplate,
  plan: planTemplate,
  "build-log": buildLogTemplate,
  idea: "",
  // sources are imported, never created from a template
  source: "",
  generic: "",
};

export function getTemplate(type: DocType): string {
  return templates[type];
}
