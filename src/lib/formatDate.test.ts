import { describe, it, expect } from "vitest";
import { formatDate } from "$lib/formatDate";

describe("formatDate", () => {
  it("returns 'Just now' for timestamps < 1 minute ago", () => {
    const now = new Date();
    expect(formatDate(now.toISOString())).toBe("Just now");
  });

  it("returns 'Xm ago' for minutes under an hour", () => {
    const d = new Date(Date.now() - 5 * 60_000);
    expect(formatDate(d.toISOString())).toBe("5m ago");
  });

  it("returns 'Xh ago' for hours under a day", () => {
    const d = new Date(Date.now() - 3 * 3_600_000);
    expect(formatDate(d.toISOString())).toBe("3h ago");
  });

  it("returns 'Xd ago' for days under a week", () => {
    const d = new Date(Date.now() - 4 * 86_400_000);
    expect(formatDate(d.toISOString())).toBe("4d ago");
  });

  it("returns a month/day format for 7+ days", () => {
    const d = new Date(Date.now() - 30 * 86_400_000);
    const result = formatDate(d.toISOString());
    // format is "Mon DD" e.g. "May 4" — assert it matches the pattern
    expect(result).toMatch(/^[A-Z][a-z]{2} \d{1,2}$/);
  });

  it("does not return negative values for future timestamps", () => {
    const future = new Date(Date.now() + 10_000);
    const result = formatDate(future.toISOString());
    expect(result).toBe("Just now");
  });
});
