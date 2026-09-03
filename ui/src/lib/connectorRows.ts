import { newId, type LabelGridRow } from "./labelGrid";
import type { CellValue, FieldSpec, LabelRowResult } from "../api/connectors";
import type { InputSpec, ParamValue } from "../api/types";

// One shared helper for displaying cell text across the browser, filters, sort, and grid.
export function displayCellText(value: CellValue | ParamValue | undefined): string {
  if (value === undefined) return "";
  if (Array.isArray(value)) return value.join(", ");
  return String(value);
}

// Maps a template field name -> a connector field key (or "" to leave the field blank).
export type FieldMapping = Record<string, string>;

// Pre-fill the mapping: a template field is mapped to a connector column of the same key only when
// the column's multi_valued matches whether the parameter is declared list.
export function defaultMapping(
  templateFields: InputSpec[],
  connectorFields: FieldSpec[],
): FieldMapping {
  const colMap = new Map<string, boolean>();
  for (const c of connectorFields) {
    colMap.set(c.key, c.multi_valued);
  }

  const mapping: FieldMapping = {};
  for (const f of templateFields) {
    const isList = f.control === "list";
    if (colMap.has(f.name) && colMap.get(f.name) === isList) {
      mapping[f.name] = f.name;
    } else {
      mapping[f.name] = "";
    }
  }
  return mapping;
}

// Reports each (parameter, column) pair whose cardinality does not match.
export function validateMapping(
  mapping: FieldMapping,
  templateFields: InputSpec[],
  connectorFields: FieldSpec[],
): string[] {
  const colMap = new Map<string, boolean>();
  for (const c of connectorFields) {
    colMap.set(c.key, c.multi_valued);
  }

  const inputMap = new Map<string, boolean>();
  for (const f of templateFields) {
    inputMap.set(f.name, f.control === "list");
  }

  const errors: string[] = [];
  for (const [paramName, colKey] of Object.entries(mapping)) {
    if (!colKey) continue;
    const isListParam = inputMap.get(paramName);
    const isMultiCol = colMap.get(colKey);
    if (isListParam === undefined || isMultiCol === undefined) continue;
    if (isMultiCol && !isListParam) {
      errors.push(
        `Cannot map multi-valued column "${colKey}" to scalar parameter "${paramName}".`,
      );
    } else if (!isMultiCol && isListParam) {
      errors.push(
        `Cannot map scalar column "${colKey}" to list parameter "${paramName}".`,
      );
    }
  }
  return errors;
}

// The distinct connector field keys to request from /materialize (drops unmapped fields).
export function mappedConnectorKeys(mapping: FieldMapping): string[] {
  return [...new Set(Object.values(mapping).filter((key) => key !== ""))];
}

// Turn materialized rows into editable grid rows, applying the field mapping. Each row keeps its
// connector source so a later batch can trace back to the Homebox entity.
export function rowsFromMaterialized(
  results: LabelRowResult[],
  mapping: FieldMapping,
  connector: string,
  connection: string,
): LabelGridRow[] {
  return results.map((result) => {
    const data: Record<string, ParamValue> = {};
    for (const [field, key] of Object.entries(mapping)) {
      if (key) {
        const val = result.data[key];
        data[field] = val !== undefined ? val : "";
      } else {
        data[field] = "";
      }
    }
    return {
      id: newId(),
      origin: "connector",
      source: { connector, connection, resource: result.source.resource, key: result.source.key },
      data,
      validation: {},
    };
  });
}
