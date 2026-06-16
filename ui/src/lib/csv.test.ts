import { describe, it, expect } from "vitest";
import { parseCsv, MAX_CSV_BYTES } from "./csv";

describe("parseCsv", () => {
  it("splits data fields from option.<name> columns and maps each row", () => {
    const r = parseCsv("sku,name,option.color\n1,Widget,red\n2,Gadget,blue\n");
    expect(r.fields).toEqual(["sku", "name"]);
    expect(r.optionColumns).toEqual(["color"]);
    expect(r.rows).toEqual([
      { data: { sku: "1", name: "Widget" }, option: { color: "red" } },
      { data: { sku: "2", name: "Gadget" }, option: { color: "blue" } },
    ]);
    expect(r.issues).toEqual([]);
  });

  it("strips a leading BOM from the first header", () => {
    const r = parseCsv("﻿sku,name\n1,Widget\n");
    expect(r.fields).toEqual(["sku", "name"]);
  });

  it("handles quoted fields with embedded commas", () => {
    const r = parseCsv('sku,name\n1,"Widget, large"\n');
    expect(r.rows[0].data.name).toBe("Widget, large");
  });

  it("flags empty and duplicate headers", () => {
    const r = parseCsv("sku,,sku\n1,2,3\n");
    expect(r.issues.join(" ")).toMatch(/empty header/i);
    expect(r.issues.join(" ")).toMatch(/duplicate header/i);
  });

  it("flags a ragged row but still maps the present cells", () => {
    const r = parseCsv("sku,name\n1\n");
    expect(r.issues.join(" ")).toMatch(/row 1/i);
    expect(r.rows[0].data).toEqual({ sku: "1", name: "" });
  });

  it("returns an error issue and no rows for an empty document", () => {
    const r = parseCsv("");
    expect(r.rows).toEqual([]);
    expect(r.issues.join(" ")).toMatch(/no rows|empty/i);
  });

  it("flags a header-only CSV with no data rows", () => {
    const r = parseCsv("sku,name\n");
    expect(r.rows).toEqual([]);
    expect(r.issues.join(" ")).toMatch(/no data rows/i);
  });

  it("marks malformed CSV (unterminated quote) as fatal", () => {
    // papaparse emits a MissingQuotes error for an opened-but-unclosed quote, which sets fatal.
    const r = parseCsv('sku\n"open');
    expect(r.fatal).toBe(true);
    expect(r.issues.join(" ")).toMatch(/parse error/i);
  });

  it("parses a valid single-column CSV without marking it fatal", () => {
    // A single-column CSV must not trip papaparse's delimiter-guess warning (delimiter is pinned to ",").
    const r = parseCsv("sku\n1\n2\n");
    expect(r.fatal).toBe(false);
    expect(r.rows).toEqual([
      { data: { sku: "1" }, option: {} },
      { data: { sku: "2" }, option: {} },
    ]);
  });

  it("rejects input larger than the size cap", () => {
    const big = "a,b\n" + "x,y\n".repeat(MAX_CSV_BYTES);
    const r = parseCsv(big);
    expect(r.issues.join(" ")).toMatch(/too large/i);
    expect(r.rows).toEqual([]);
  });
});
