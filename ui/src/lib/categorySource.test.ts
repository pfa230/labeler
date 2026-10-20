import { describe, expect, it } from "vitest";
import { parse } from "yaml";
import { editCategories, readCategories } from "./categorySource";

describe("readCategories", () => {
  it("reads a block list as separate strings", () => {
    const source = "id: pallet\ncategories:\n  - Shipping\n  - Warehouse\nname: x\n";
    expect(readCategories(source)).toEqual({ ok: true, categories: ["Shipping", "Warehouse"] });
  });

  it("reads a flow list", () => {
    expect(readCategories("categories: [Shipping, Warehouse]\n")).toEqual({
      ok: true,
      categories: ["Shipping", "Warehouse"],
    });
  });

  it("reads an empty list", () => {
    expect(readCategories("categories: []\n")).toEqual({ ok: true, categories: [] });
  });

  it("reads an absent key as an empty list", () => {
    expect(readCategories("id: pallet\nname: Pallet\n")).toEqual({ ok: true, categories: [] });
  });

  it("refuses a shared anchor reused through an alias", () => {
    const source = "base: &c [Old]\ncategories: *c\n";
    expect(readCategories(source)).toEqual({ ok: false, reason: expect.any(String) });
  });

  it("refuses an explicit tag on the list", () => {
    expect(readCategories("categories: !foo [A]\n")).toEqual({
      ok: false,
      reason: expect.any(String),
    });
  });

  it("refuses another collection's alias", () => {
    expect(readCategories("categories: *missing\n")).toEqual({
      ok: false,
      reason: expect.any(String),
    });
  });

  it("refuses invalid YAML", () => {
    expect(readCategories("id: [foo\n")).toEqual({ ok: false, reason: expect.any(String) });
  });

  it("refuses duplicate keys", () => {
    expect(readCategories("id: a\nid: b\ncategories: [A]\n")).toEqual({
      ok: false,
      reason: expect.any(String),
    });
  });

  it("refuses a non-mapping root", () => {
    expect(readCategories("- categories: [A]\n")).toEqual({
      ok: false,
      reason: expect.any(String),
    });
  });
});

describe("editCategories", () => {
  it("replaces a block list, keeping every other byte", () => {
    const source =
      "id: pallet\nname: Pallet\ncategories:\n  - Shipping\n  - Warehouse\ndescription: keep me\n";
    const result = editCategories(source, ["A", "B"]);
    expect(result).toEqual({
      ok: true,
      source: 'id: pallet\nname: Pallet\ncategories:\n  ["A","B"]\ndescription: keep me\n',
    });
    if (!result.ok) throw new Error("expected an edit");
    // A whole-document reserialization would reorder the keys and reformat the block list.
    expect(parse(result.source)).toEqual({
      id: "pallet",
      name: "Pallet",
      categories: ["A", "B"],
      description: "keep me",
    });
  });

  it("replaces a flow list", () => {
    const source = "id: pallet\ncategories: [Shipping, Warehouse]\ndescription: keep me\n";
    const result = editCategories(source, ["A", "B"]);
    expect(result).toEqual({
      ok: true,
      source: 'id: pallet\ncategories: ["A","B"]\ndescription: keep me\n',
    });
  });

  it("clears a flow list to an empty list", () => {
    const result = editCategories("id: pallet\ncategories: [Shipping]\nname: x\n", []);
    expect(result).toEqual({ ok: true, source: "id: pallet\ncategories: []\nname: x\n" });
  });

  it("clears a block list to an empty list", () => {
    const result = editCategories("id: pallet\ncategories:\n  - Shipping\nname: x\n", []);
    expect(result).toEqual({ ok: true, source: "id: pallet\ncategories:\n  []\nname: x\n" });
    if (!result.ok) throw new Error("expected an edit");
    expect(readCategories(result.source)).toEqual({ ok: true, categories: [] });
  });

  it("inserts an absent key before the first block-map entry", () => {
    const source = "id: pallet\nname: Pallet\ndescription: x\n";
    const result = editCategories(source, ["A", "B"]);
    expect(result).toEqual({
      ok: true,
      source: 'categories: ["A","B"]\nid: pallet\nname: Pallet\ndescription: x\n',
    });
    if (!result.ok) throw new Error("expected an edit");
    expect(readCategories(result.source)).toEqual({ ok: true, categories: ["A", "B"] });
  });

  it("inserts into a flow root with the required separator", () => {
    const result = editCategories("{ id: pallet, name: Pallet }", ["A", "B"]);
    expect(result).toEqual({
      ok: true,
      source: '{ categories: ["A","B"], id: pallet, name: Pallet }',
    });
  });

  it("handles a quoted categories key", () => {
    const source = 'id: pallet\n"categories":\n  - A\ndescription: x\n';
    const result = editCategories(source, ["A", "B"]);
    expect(result).toEqual({
      ok: true,
      source: 'id: pallet\n"categories":\n  ["A","B"]\ndescription: x\n',
    });
  });

  it("keeps a leading document marker", () => {
    const source = "---\nid: pallet\ncategories:\n  - A\nname: x\n";
    const result = editCategories(source, ["A", "B"]);
    expect(result).toEqual({
      ok: true,
      source: '---\nid: pallet\ncategories:\n  ["A","B"]\nname: x\n',
    });
  });

  it("inserts at the indentation of an indented root map", () => {
    const source = "  id: pallet\n  name: Pallet\n";
    const result = editCategories(source, ["A"]);
    expect(result).toEqual({
      ok: true,
      source: '  categories: ["A"]\n  id: pallet\n  name: Pallet\n',
    });
  });

  it("preserves CRLF line endings", () => {
    const source = "id: pallet\r\ncategories:\r\n  - A\r\n  - B\r\nname: x\r\n";
    const result = editCategories(source, ["A", "B"]);
    expect(result).toEqual({
      ok: true,
      source: 'id: pallet\r\ncategories:\r\n  ["A","B"]\r\nname: x\r\n',
    });
  });

  it("preserves Unicode and a multiline value", () => {
    const source = "id: pallet\ncategories:\n  - Café\ndesc: |\n  line one\n  line two\n";
    const result = editCategories(source, ["日本", "Zürich"]);
    expect(result).toEqual({
      ok: true,
      source: 'id: pallet\ncategories:\n  ["日本","Zürich"]\ndesc: |\n  line one\n  line two\n',
    });
    if (!result.ok) throw new Error("expected an edit");
    expect(parse(result.source)).toEqual({
      id: "pallet",
      categories: ["日本", "Zürich"],
      desc: "line one\nline two\n",
    });
  });

  it("keeps a comma-bearing and quote-bearing string as one entry", () => {
    const source = "categories: [Old]\n";
    const result = editCategories(source, ["A,B", 'He said "hi"']);
    expect(result).toEqual({
      ok: true,
      source: 'categories: ["A,B","He said \\"hi\\""]\n',
    });
    if (!result.ok) throw new Error("expected an edit");
    expect(readCategories(result.source)).toEqual({ ok: true, categories: ["A,B", 'He said "hi"'] });
  });

  it("retains a leading BOM when inserting an absent key", () => {
    const source = "\uFEFFid: pallet\nname: Pallet\n";
    const result = editCategories(source, ["A"]);
    expect(result).toEqual({
      ok: true,
      source: '\uFEFFcategories: ["A"]\nid: pallet\nname: Pallet\n',
    });
  });

  it.each([
    ["a shared anchor reused through an alias", "base: &c [Old]\ncategories: *c\n"],
    ["an anchored list", "categories: &c [Old]\n"],
    ["a tagged list", "categories: !foo [A]\n"],
    ["an anchored entry", "categories: [&a A]\n"],
    ["a tagged entry", "categories: [!t A]\n"],
    ["a scalar value", "categories: foo\n"],
    ["invalid YAML", "id: [foo\n"],
  ])("refuses %s without producing a source", (_label, source) => {
    const result = editCategories(source, ["A"]);
    expect(result).toEqual({ ok: false, reason: expect.any(String) });
  });
});
