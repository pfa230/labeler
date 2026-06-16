import { describe, it, expect } from "vitest";
import {
  MAX_BATCH_LABELS,
  expandedCount,
  resolveLabels,
  sourceRowForExpandedIndex,
  duplicateRow,
  removeRow,
  validateOptionCell,
  type LabelGridRow,
} from "./labelGrid";

function row(id: string, data: Record<string, string>, option: Record<string, string> = {}): LabelGridRow {
  return { id, origin: "csv", data, option, validation: {} };
}

describe("labelGrid logic", () => {
  it("expandedCount multiplies rows by copies", () => {
    expect(expandedCount(3, 2)).toBe(6);
    expect(expandedCount(0, 5)).toBe(0);
  });

  it("MAX_BATCH_LABELS is the backend cap", () => {
    expect(MAX_BATCH_LABELS).toBe(500);
  });

  it("resolveLabels expands copies adjacently and merges options (row wins over manual)", () => {
    const rows = [row("a", { sku: "1" }, { color: "red" }), row("b", { sku: "2" })];
    const out = resolveLabels(rows, { color: "blue", size: "L" }, 2);
    expect(out).toEqual([
      { data: { sku: "1" }, option: { color: "red", size: "L" } },
      { data: { sku: "1" }, option: { color: "red", size: "L" } },
      { data: { sku: "2" }, option: { color: "blue", size: "L" } },
      { data: { sku: "2" }, option: { color: "blue", size: "L" } },
    ]);
  });

  it("resolveLabels omits option entirely when the merged map is empty", () => {
    const out = resolveLabels([row("a", { sku: "1" })], {}, 1);
    expect(out).toEqual([{ data: { sku: "1" } }]);
    expect("option" in out[0]).toBe(false);
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

  it("validateOptionCell returns an error for a value not in the allowed set", () => {
    expect(validateOptionCell("red", ["red", "blue"])).toBeUndefined();
    expect(validateOptionCell("green", ["red", "blue"])).toMatch(/not allowed/i);
    // an undeclared option (no allowed list) is flagged
    expect(validateOptionCell("x", undefined)).toMatch(/not a declared option/i);
  });
});
