import { describe, it, expect } from "vitest";
import { defaultMapping, mappedConnectorKeys, rowsFromMaterialized } from "./connectorRows";

describe("connectorRows", () => {
  it("defaultMapping matches template fields to identically-named connector keys", () => {
    const m = defaultMapping(["name", "sku", "qty"], ["name", "qty", "manufacturer"]);
    expect(m).toEqual({ name: "name", sku: "", qty: "qty" });
  });

  it("mappedConnectorKeys returns distinct non-empty targets", () => {
    expect(mappedConnectorKeys({ a: "name", b: "name", c: "" }).sort()).toEqual(["name"]);
  });

  it("rowsFromMaterialized builds connector-origin rows with mapped data and source", () => {
    const rows = rowsFromMaterialized(
      [{ source: { resource: "entities", key: "e1" }, data: { name: "Drill", manufacturer: "Acme" } }],
      { title: "name", maker: "manufacturer", blank: "" },
      "homebox",
      "c1",
    );
    expect(rows).toHaveLength(1);
    expect(rows[0].origin).toBe("connector");
    expect(rows[0].source).toEqual({ connector: "homebox", connection: "c1", resource: "entities", key: "e1" });
    expect(rows[0].data).toEqual({ title: "Drill", maker: "Acme", blank: "" });
    expect(rows[0].option).toEqual({});
  });
});
