import { describe, it, expect } from "vitest";
import { blankOptionLabel, getOwnKey, pruneDataForSubmit, resolveImportData, setOwnKey } from "./labelInputs";
import type { Param } from "../api/types";

const param = (name: string, control: Param["control"], extra: Partial<Param> = {}): Param =>
  ({ name, type: "string", control, ...extra }) as Param;

describe("pruneDataForSubmit", () => {
  it("omits names no parameter declares even when data has a non-empty value", () => {
    const params: Param[] = [
      { name: "title", type: "string", control: "text" },
      { name: "orientation", type: "enum", control: "select", values: ["horizontal", "vertical"] },
    ];
    const data = {
      title: "My Label",
      orientation: "horizontal",
      deactivated_field: "old value",
      other_inactive: 123,
    };
    const pruned = pruneDataForSubmit(data, params);
    expect(pruned).toEqual({
      title: "My Label",
      orientation: "horizontal",
    });
    expect(pruned).not.toHaveProperty("deactivated_field");
    expect(pruned).not.toHaveProperty("other_inactive");
  });

  it("omits empty strings for integer, number, select, date and datetime controls", () => {
    const params: Param[] = [
      { name: "count", type: "integer", control: "integer" },
      { name: "price", type: "number", control: "number" },
      { name: "tier", type: "enum", control: "select", values: ["a", "b"] },
      { name: "day", type: "datetime", control: "date" },
      { name: "printed_on", type: "datetime", control: "datetime", time: true },
    ];
    const data = {
      count: "",
      price: "",
      tier: "",
      day: "",
      printed_on: "",
    };
    expect(pruneDataForSubmit(data, params)).toEqual({});
  });

  it("omits every blank control, defaulted or not", () => {
    const params = [
      param("title", "text", { default: "Untitled" }),
      param("note", "text"),
      param("body", "textarea"),
      param("photo", "image"),
      param("qty", "integer", { type: "integer" }),
      param("when", "date", { type: "datetime", default: "2026-01-01" }),
    ];
    // title cleared to "", the rest never touched
    expect(pruneDataForSubmit({ title: "" }, params)).toEqual({});
  });

  it("sends a held list, [] included, and nothing for an untouched one (guard)", () => {
    const params = [param("a", "list", { type: "list" }), param("b", "list", { type: "list", default: ["X"] })];
    expect(pruneDataForSubmit({ a: [] }, params)).toEqual({ a: [] });
    expect(pruneDataForSubmit({}, params)).toEqual({});
  });

  it("reads and writes names that shadow Object.prototype as own keys, in declaration order", () => {
    const params = [param("constructor", "checkbox", { type: "boolean" }), param("__proto__", "text")];
    const untouched = pruneDataForSubmit({}, params);
    expect(Object.keys(untouched)).toEqual(["constructor"]);
    expect(untouched.constructor).toBe(false);
    const data: Record<string, unknown> = {};
    setOwnKey(data, "__proto__", "typed");
    const typed = pruneDataForSubmit(data, params);
    expect(Object.keys(typed)).toEqual(["constructor", "__proto__"]);
    expect(getOwnKey(typed, "__proto__")).toBe("typed");
  });

  it("sends a checkbox's start state when its key is absent or blank, and a held value unchanged", () => {
    const params: Param[] = [
      { name: "absent", type: "boolean", control: "checkbox" },
      { name: "blank", type: "boolean", control: "checkbox", default: true },
      { name: "held_off", type: "boolean", control: "checkbox", default: true },
      { name: "unreadable", type: "boolean", control: "checkbox" },
    ];
    const data = { blank: "", held_off: "false", unreadable: "yes" };
    expect(pruneDataForSubmit(data, params)).toEqual({
      absent: false,
      blank: true,
      held_off: "false",
      unreadable: "yes",
    });
  });
});

describe("resolveImportData", () => {
  const listParam = (name = "tags"): Param => ({ name, type: "list", control: "list" });

  it("decodes a declared list cell as a JSON array of strings, keeping order, duplicates and empty elements", () => {
    const { data, errors } = resolveImportData({ tags: '["A","","B,C","A"]' }, [listParam()]);
    expect(errors).toEqual([]);
    expect(data).toEqual({ tags: ["A", "", "B,C", "A"] });
  });

  it("applies normal JSON escapes inside the array", () => {
    const { data, errors } = resolveImportData({ tags: '["line\\nbreak","quote\\"d"]' }, [listParam()]);
    expect(errors).toEqual([]);
    expect(data).toEqual({ tags: ["line\nbreak", 'quote"d'] });
  });

  it("omits an empty or absent list cell, and distinguishes [] from [\"\"]", () => {
    expect(resolveImportData({ tags: "" }, [listParam()]).data).toEqual({});
    expect(resolveImportData({}, [listParam()]).data).toEqual({});
    expect(resolveImportData({ tags: "[]" }, [listParam()]).data).toEqual({ tags: [] });
    expect(resolveImportData({ tags: '[""]' }, [listParam()]).data).toEqual({ tags: [""] });
  });

  it("reports a value that is not an array of strings against its field and omits it", () => {
    expect(resolveImportData({ tags: "[1]" }, [listParam()])).toEqual({
      data: {},
      errors: [{ field: "tags", message: "must be a JSON array of strings" }],
    });
    expect(resolveImportData({ tags: '"x"' }, [listParam()]).errors).toHaveLength(1);
    expect(resolveImportData({ tags: '[["a"]]' }, [listParam()]).errors).toHaveLength(1);
  });

  it("reports malformed JSON and nonempty whitespace", () => {
    expect(resolveImportData({ tags: "[" }, [listParam()]).errors).toEqual([
      { field: "tags", message: "is not valid JSON" },
    ]);
    expect(resolveImportData({ tags: "   " }, [listParam()]).errors).toHaveLength(1);
  });

  it("keeps pruneDataForSubmit's rules for every other control", () => {
    const params: Param[] = [
      { name: "title", type: "string", control: "text" },
      { name: "flag", type: "boolean", control: "checkbox", default: true },
      listParam(),
    ];
    const { data, errors } = resolveImportData({ title: "", flag: "", tags: '["x"]', extra: "drop" }, params);
    expect(errors).toEqual([]);
    expect(data).toEqual({ flag: true, tags: ["x"] });
  });
});

describe("blankOptionLabel", () => {
  it("shows the default when there is one, else nothing", () => {
    expect(blankOptionLabel({ default: "vertical" })).toBe("(default: vertical)");
    expect(blankOptionLabel({})).toBe("");
  });
});
