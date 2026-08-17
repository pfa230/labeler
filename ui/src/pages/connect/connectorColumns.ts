import type { FieldSpec } from "../../api/connectors";

export function defaultColumnKeys(columns: FieldSpec[]): Set<string> {
  const cheap = columns.filter((c) => c.tier === "cheap").map((c) => c.key);
  if (cheap.length > 0) {
    return new Set(cheap);
  }
  return new Set(columns.map((c) => c.key));
}

export function loadSavedColumnKeys(
  connectionId: string,
  resourceId: string,
  columns: FieldSpec[]
): Set<string> {
  if (typeof window === "undefined" || !window.localStorage) {
    return defaultColumnKeys(columns);
  }
  try {
    const raw = window.localStorage.getItem(`labeler:connector-columns:${connectionId}:${resourceId}`);
    if (raw) {
      const parsed: string[] = JSON.parse(raw);
      const valid = new Set(columns.map((c) => c.key));
      const filtered = parsed.filter((k) => valid.has(k));
      if (filtered.length > 0) {
        return new Set(filtered);
      }
    }
  } catch {
    // Ignore storage parse errors
  }
  return defaultColumnKeys(columns);
}

export function saveColumnKeys(
  connectionId: string,
  resourceId: string,
  keys: Set<string>
): void {
  if (typeof window === "undefined" || !window.localStorage) return;
  try {
    window.localStorage.setItem(
      `labeler:connector-columns:${connectionId}:${resourceId}`,
      JSON.stringify(Array.from(keys))
    );
  } catch {
    // Ignore storage quota/security errors
  }
}
