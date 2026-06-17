import { newId, type LabelGridRow } from "./labelGrid";
import type { LabelRowResult } from "../api/connectors";

// Maps a template field name -> a connector field key (or "" to leave the field blank).
export type FieldMapping = Record<string, string>;

// Pre-fill the mapping: a template field is mapped to a connector column of the same key when one exists.
export function defaultMapping(templateFields: string[], connectorFieldKeys: string[]): FieldMapping {
  const available = new Set(connectorFieldKeys);
  const mapping: FieldMapping = {};
  for (const field of templateFields) mapping[field] = available.has(field) ? field : "";
  return mapping;
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
    const data: Record<string, string> = {};
    for (const [field, key] of Object.entries(mapping)) {
      data[field] = key ? (result.data[key] ?? "") : "";
    }
    return {
      id: newId(),
      origin: "connector",
      source: { connector, connection, resource: result.source.resource, key: result.source.key },
      data,
      option: {},
      validation: {},
    };
  });
}
