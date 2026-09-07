import type { FieldSpec } from "../../api/connectors";

export interface ColumnChoice {
  visible: string[];
  hiddenDerived: string[];
}

export function isTransformDerived(column: FieldSpec): boolean {
  return column.tier === "derived" && !column.transform_source;
}

export function defaultColumnKeys(columns: FieldSpec[]): Set<string> {
  const opening = columns
    .filter((c) => c.tier === "cheap" || isTransformDerived(c))
    .map((c) => c.key);
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

  const validKeys = new Set(columns.map((c) => c.key));
  const hiddenDerivedSet = new Set(choice.hiddenDerived);
  const visibleSet = new Set(choice.visible.filter((k) => validKeys.has(k)));

  const result: string[] = [];
  for (const c of columns) {
    if (isTransformDerived(c)) {
      if (!hiddenDerivedSet.has(c.key)) {
        result.push(c.key);
      }
    } else {
      if (visibleSet.has(c.key)) {
        result.push(c.key);
      }
    }
  }

  if (result.length === 0) {
    return defaultColumnKeys(columns);
  }
  return new Set(result);
}

export function makeColumnChoice(
  columns: FieldSpec[],
  visibleKeys: Set<string> | string[]
): ColumnChoice {
  const visibleArr = Array.from(visibleKeys);
  const visibleSet = new Set(visibleArr);
  const hiddenDerived: string[] = [];
  for (const c of columns) {
    if (isTransformDerived(c) && !visibleSet.has(c.key)) {
      hiddenDerived.push(c.key);
    }
  }
  return {
    visible: visibleArr,
    hiddenDerived,
  };
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
      if (Array.isArray(parsed)) {
        return {
          visible: parsed.filter((k): k is string => typeof k === "string"),
          hiddenDerived: [],
        };
      }
      if (parsed && typeof parsed === "object" && Array.isArray(parsed.visible)) {
        return {
          visible: parsed.visible.filter((k: unknown): k is string => typeof k === "string"),
          hiddenDerived: Array.isArray(parsed.hiddenDerived)
            ? parsed.hiddenDerived.filter((k: unknown): k is string => typeof k === "string")
            : [],
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
