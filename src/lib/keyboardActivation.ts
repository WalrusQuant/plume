export function shouldActivateFromKey(event: Pick<KeyboardEvent, "key" | "target" | "currentTarget">): boolean {
  return event.target === event.currentTarget && (event.key === "Enter" || event.key === " ");
}
