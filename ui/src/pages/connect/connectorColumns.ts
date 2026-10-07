import type { FieldSpec } from "../../api/connectors";

export interface ColumnChoice {
  visible: string[];
}

export function defaultColumnKeys(columns: FieldSpec[]): Set<string> {
  const opening = columns.filter((c) => c.tier === "cheap").map((c) => c.key);
  if (opening.length > 0) {
    return new Set(opening);
  }
  return new Set(columns.map((c) => c.key));
}

export function resolveColumnKeys(
  columns: FieldSpec[],
  choice?: ColumnChoice | null
): Set<string> {
  if (!choice) {
    return defaultColumnKeys(columns);
  }

  const visibleSet = new Set(choice.visible);
  const result = columns.filter((c) => visibleSet.has(c.key)).map((c) => c.key);

  if (result.length === 0) {
    return defaultColumnKeys(columns);
  }
  return new Set(result);
}

export function makeColumnChoice(visibleKeys: Set<string> | string[]): ColumnChoice {
  return { visible: Array.from(visibleKeys) };
}

export function loadSavedColumnChoice(
  connectionId: string,
  resourceId: string
): ColumnChoice | null {
  if (typeof window === "undefined" || !window.localStorage) {
    return null;
  }
  try {
    const raw = window.localStorage.getItem(
      `labeler:connector-columns:${connectionId}:${resourceId}`
    );
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed && typeof parsed === "object" && Array.isArray(parsed.visible)) {
        return {
          visible: parsed.visible.filter((k: unknown): k is string => typeof k === "string"),
        };
      }
    }
  } catch {
    // Ignore storage parse errors
  }
  return null;
}

export function saveColumnChoice(
  connectionId: string,
  resourceId: string,
  choice: ColumnChoice
): void {
  if (typeof window === "undefined" || !window.localStorage) return;
  try {
    window.localStorage.setItem(
      `labeler:connector-columns:${connectionId}:${resourceId}`,
      JSON.stringify(choice)
    );
  } catch {
    // Ignore storage quota/security errors
  }
}
