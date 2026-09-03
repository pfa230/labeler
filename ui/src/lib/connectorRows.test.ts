import { describe, it, expect } from "vitest";
import { defaultMapping, mappedConnectorKeys, rowsFromMaterialized, validateMapping, displayCellText } from "./connectorRows";

describe("connectorRows", () => {
  it("defaultMapping matches template fields to identically-named connector keys", () => {
    const inputs = [
      { name: "name", control: "text" as const },
      { name: "sku", control: "text" as const },
      { name: "qty", control: "text" as const },
    ];
    const columns = [
      { key: "name", label: "Name", ty: "text" as const, tier: "cheap" as const, multi_valued: false },
      { key: "qty", label: "Qty", ty: "text" as const, tier: "cheap" as const, multi_valued: false },
      { key: "manufacturer", label: "Manufacturer", ty: "text" as const, tier: "cheap" as const, multi_valued: false },
    ];
    const m = defaultMapping(inputs, columns);
    expect(m).toEqual({ name: "name", sku: "", qty: "qty" });
  });

  it("defaultMapping considers cardinality: leaves string parameter unmapped for multi-valued column, pre-fills list parameter", () => {
    const scalarInput = { name: "tags", control: "text" as const };
    const listInput = { name: "tags", control: "list" as const };
    const multiCol = { key: "tags", label: "Tags", ty: "text" as const, tier: "cheap" as const, multi_valued: true };

    const mScalar = defaultMapping([scalarInput], [multiCol]);
    expect(mScalar).toEqual({ tags: "" });

    const mList = defaultMapping([listInput], [multiCol]);
    expect(mList).toEqual({ tags: "tags" });
  });

  it("validateMapping reports mismatch in both directions naming column and parameter, and nothing for valid mapping", () => {
    const inputs = [
      { name: "scalarParam", control: "text" as const },
      { name: "listParam", control: "list" as const },
      { name: "unmappedParam", control: "list" as const },
    ];
    const columns = [
      { key: "multiCol", label: "Multi", ty: "text" as const, tier: "cheap" as const, multi_valued: true },
      { key: "scalarCol", label: "Scalar", ty: "text" as const, tier: "cheap" as const, multi_valued: false },
    ];

    // Mismatches in both directions:
    const badMapping = {
      scalarParam: "multiCol",
      listParam: "scalarCol",
      unmappedParam: "",
    };
    const errors = validateMapping(badMapping, inputs, columns);
    expect(errors).toHaveLength(2);
    expect(errors[0]).toContain("multiCol");
    expect(errors[0]).toContain("scalarParam");
    expect(errors[1]).toContain("scalarCol");
    expect(errors[1]).toContain("listParam");

    // Valid mapping:
    const goodMapping = {
      scalarParam: "scalarCol",
      listParam: "multiCol",
      unmappedParam: "",
    };
    expect(validateMapping(goodMapping, inputs, columns)).toEqual([]);
  });

  it("mappedConnectorKeys returns distinct non-empty targets", () => {
    expect(mappedConnectorKeys({ a: "name", b: "name", c: "" }).sort()).toEqual(["name"]);
  });

  it("rowsFromMaterialized builds connector-origin rows with mapped data and source", () => {
    const rows = rowsFromMaterialized(
      [{ source: { resource: "entities", key: "e1" }, data: { name: "Drill", manufacturer: "Acme", tags: ["KIDS", "CONSUMABLE"] } }],
      { title: "name", maker: "manufacturer", tags: "tags", blank: "" },
      "homebox",
      "c1",
    );
    expect(rows).toHaveLength(1);
    expect(rows[0].origin).toBe("connector");
    expect(rows[0].source).toEqual({ connector: "homebox", connection: "c1", resource: "entities", key: "e1" });
    expect(rows[0].data).toEqual({ title: "Drill", maker: "Acme", tags: ["KIDS", "CONSUMABLE"], blank: "" });
    expect(rows[0].option).toEqual({});
  });

  it("displayCellText formats absent, string, number, and array correctly", () => {
    expect(displayCellText(undefined)).toBe("");
    expect(displayCellText("")).toBe("");
    expect(displayCellText("hello")).toBe("hello");
    expect(displayCellText(42)).toBe("42");
    expect(displayCellText([])).toBe("");
    expect(displayCellText(["KIDS", "CONSUMABLE"])).toBe("KIDS, CONSUMABLE");
  });
});
