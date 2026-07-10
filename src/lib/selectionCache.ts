const selectionKey = (pipelineId: number) => `branchgate-selection-${pipelineId}`;

export function loadCachedSelection(pipelineId: number): number[] | null {
  const raw = localStorage.getItem(selectionKey(pipelineId));
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return null;
    return parsed.filter((id): id is number => typeof id === "number");
  } catch {
    return null;
  }
}

export function saveCachedSelection(pipelineId: number, prIds: number[]) {
  localStorage.setItem(selectionKey(pipelineId), JSON.stringify(prIds));
}

export function applyCachedSelection(
  pipelineId: number,
  pendingPrIds: number[],
): Set<number> {
  const cached = loadCachedSelection(pipelineId);
  if (cached === null) {
    return new Set(pendingPrIds);
  }
  const pending = new Set(pendingPrIds);
  return new Set(cached.filter((id) => pending.has(id)));
}

export function clearCachedSelection(pipelineId: number) {
  localStorage.removeItem(selectionKey(pipelineId));
}
