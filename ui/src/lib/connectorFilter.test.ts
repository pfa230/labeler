import { describe, it, expect } from "vitest";
import { matchesFilters } from "./connectorFilter";
import type { DisplayRow } from "../api/connectors";

function row(cells: DisplayRow["cells"]): DisplayRow {
  return { id: { resource: "items", key: "1" }, cells };
}

describe("matchesFilters", () => {
  it("matches case-insensitively", () => {
    expect(matchesFilters(row({ name: "WIDGET" }), { name: "widget" })).toBe(true);
    expect(matchesFilters(row({ name: "Widget" }), { name: "widget" })).toBe(true);
  });

  it("does not match a genuinely different needle", () => {
    expect(matchesFilters(row({ name: "Widget" }), { name: "gadget" })).toBe(false);
  });

  it("matches a needle in the middle of a cell", () => {
    expect(matchesFilters(row({ name: "Blue Widget Large" }), { name: "Widget" })).toBe(true);
  });

  it("matches a needle at the end of a cell", () => {
    expect(matchesFilters(row({ name: "Blue Widget" }), { name: "Widget" })).toBe(true);
  });

  it("requires every non-empty filter to match (AND across columns)", () => {
    const r = row({ name: "Widget", sku: "abc123" });
    expect(matchesFilters(r, { name: "Widget", sku: "abc" })).toBe(true);
  });

  it("fails a row satisfying only one of two filters", () => {
    const r = row({ name: "Widget", sku: "abc123" });
    expect(matchesFilters(r, { name: "Widget", sku: "zzz" })).toBe(false);
  });

  it("does not restrict on an empty needle for one column", () => {
    const r = row({ name: "Widget", sku: "abc123" });
    expect(matchesFilters(r, { name: "", sku: "zzz" })).toBe(false);
    expect(matchesFilters(r, { name: "", sku: "abc" })).toBe(true);
  });

  it("passes every row when all filters are empty", () => {
    expect(matchesFilters(row({ name: "Widget" }), { name: "", sku: "" })).toBe(true);
  });

  it("fails a non-empty filter on a column absent from the row's cells", () => {
    expect(matchesFilters(row({ name: "Widget" }), { sku: "abc" })).toBe(false);
    expect(matchesFilters(row({ name: "Widget" }), { sku: "undefined" })).toBe(false);
  });

  it("passes an empty filter on a column absent from the row's cells", () => {
    expect(matchesFilters(row({ name: "Widget" }), { sku: "" })).toBe(true);
  });

  it("finds a number column whose displayed value is the text n/a", () => {
    expect(matchesFilters(row({ qty: "n/a" }), { qty: "n/a" })).toBe(true);
  });

  it("matches a numeric cell by its string form", () => {
    expect(matchesFilters(row({ qty: 10 }), { qty: "1" })).toBe(true);
    expect(matchesFilters(row({ qty: 10 }), { qty: "10" })).toBe(true);
  });

  it("treats a whitespace-only needle as no filter", () => {
    expect(matchesFilters(row({ name: "Widget" }), { name: "   " })).toBe(true);
    expect(matchesFilters(row({ name: "anything" }), { name: "   " })).toBe(true);
  });

  it("trims incidental surrounding spaces from a needle before matching", () => {
    expect(matchesFilters(row({ name: "Widget" }), { name: "  Widget  " })).toBe(true);
  });

  it("matches a needle in a multi-valued cell case-insensitively", () => {
    const r = row({ tags: ["KIDS", "CONSUMABLE"] });
    expect(matchesFilters(r, { tags: "kids" })).toBe(true);
    expect(matchesFilters(r, { tags: "CONSUMABLE" })).toBe(true);
    expect(matchesFilters(r, { tags: "s, con" })).toBe(true);
    expect(matchesFilters(r, { tags: "adult" })).toBe(false);
  });

  it("does not match a non-empty needle against an empty array multi-valued cell", () => {
    const r = row({ tags: [] });
    expect(matchesFilters(r, { tags: "kids" })).toBe(false);
    expect(matchesFilters(r, { tags: "" })).toBe(true);
  });
});
