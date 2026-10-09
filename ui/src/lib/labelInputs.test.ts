import { describe, it, expect } from "vitest";
import { pruneDataForSubmit } from "./labelInputs";
import type { Param } from "../api/types";

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

  it("submits empty strings for text, textarea, and image controls", () => {
    const params: Param[] = [
      { name: "t", type: "string", control: "text" },
      { name: "ta", type: "string", control: "textarea" },
      { name: "img", type: "string", control: "image" },
    ];
    const data = { t: "", ta: "", img: "" };
    expect(pruneDataForSubmit(data, params)).toEqual({
      t: "",
      ta: "",
      img: "",
    });
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

  it("omits deferred names from the result while retaining non-deferred names", () => {
    const params: Param[] = [
      { name: "title", type: "string", control: "text", default: "Untitled" },
      { name: "count", type: "integer", control: "integer", default: 1 },
      { name: "notes", type: "string", control: "text" },
    ];
    const data = {
      title: "Untitled",
      count: 1,
      notes: "Custom note",
    };
    const deferred = {
      title: true,
      count: false,
    };
    const pruned = pruneDataForSubmit(data, params, deferred);
    expect(pruned).toEqual({
      count: 1,
      notes: "Custom note",
    });
    expect(pruned).not.toHaveProperty("title");
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
