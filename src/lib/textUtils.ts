/** Strip a single code fence that wraps the entire text — a weaker model may
    return the replacement fenced despite the prompt asking for raw text. Only
    strips when the fence wraps everything, so genuinely fenced code survives. */
export function stripWrappingFence(s: string): string {
  const m = s.trim().match(/^```[^\n]*\n([\s\S]*?)\n```$/);
  return m ? m[1] : s;
}
