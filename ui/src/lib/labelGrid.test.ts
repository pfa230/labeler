import { describe, it, expect } from "vitest";
import {
  MAX_BATCH_LABELS,
  expandedCount,
  resolveLabels,
  sourceRowForExpandedIndex,
  duplicateRow,
  removeRow,
  type LabelGridRow,
} from "./labelGrid";

function row(id: string, data: Record<string, string>): LabelGridRow {
  return { id, origin: "csv", data, validation: {} };
}

describe("labelGrid logic", () => {
  it("expandedCount multiplies rows by copies", () => {
    expect(expandedCount(3, 2)).toBe(6);
    expect(expandedCount(0, 5)).toBe(0);
  });

  it("MAX_BATCH_LABELS is the backend cap", () => {
    expect(MAX_BATCH_LABELS).toBe(500);
  });

  it("resolveLabels expands copies adjacently", () => {
    const rows = [row("a", { sku: "1" }), row("b", { sku: "2" })];
    const out = resolveLabels(rows, 2);
    expect(out).toEqual([
      { data: { sku: "1" } },
      { data: { sku: "1" } },
      { data: { sku: "2" } },
      { data: { sku: "2" } },
    ]);
  });

  it("sourceRowForExpandedIndex maps an expanded index back to its source row", () => {
    // 2 rows x 3 copies => [0,0,0,1,1,1]
    expect(sourceRowForExpandedIndex(0, 3)).toBe(0);
    expect(sourceRowForExpandedIndex(2, 3)).toBe(0);
    expect(sourceRowForExpandedIndex(3, 3)).toBe(1);
    expect(sourceRowForExpandedIndex(5, 3)).toBe(1);
  });

  it("duplicateRow inserts a copy right after the source with a new id and shared copyGroup", () => {
    const rows = [row("a", { sku: "1" }), row("b", { sku: "2" })];
    const out = duplicateRow(rows, "a");
    expect(out).toHaveLength(3);
    expect(out[1].id).not.toBe("a");
    expect(out[1].data).toEqual({ sku: "1" });
    expect(out[1].copyGroup).toBeDefined();
    expect(out[0].copyGroup).toBe(out[1].copyGroup);
  });

  it("removeRow drops the row by id", () => {
    const rows = [row("a", { sku: "1" }), row("b", { sku: "2" })];
    expect(removeRow(rows, "a")).toEqual([rows[1]]);
  });
});
