export function isMacPlatform(): boolean {
  return /mac/i.test(navigator.platform) || /mac/i.test(navigator.userAgent);
}

export function modLabel(): string {
  return isMacPlatform() ? "⌘" : "Ctrl";
}

export function modKeyActive(event: KeyboardEvent): boolean {
  return isMacPlatform() ? event.metaKey : event.ctrlKey;
}

export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  return target.isContentEditable;
}

export function formatShortcut(keys: string[]): string {
  const mod = modLabel();
  return keys.map((k) => (k === "Mod" ? mod : k)).join(isMacPlatform() ? "" : "+");
}
