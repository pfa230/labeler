import { describe, it, expect } from "vitest";
import { referencedFields, referencedSettings, defaultOptions, imageFields, reconcileRowOptions } from "./templateFields";
import type { LayoutItem, Options } from "../api/types";

const layout: LayoutItem[] = [
  { type: "text", name: "title" },
  { type: "qr", value: "{settings.qr_base_url}/{id}" },
  { type: "image", name: "logo" },
  { type: "text", value: "literal {{not a field}}" },
  { type: "container", option: { orientation: "horizontal" }, items: [{ type: "text", name: "h_only" }] },
  { type: "container", option: { orientation: "vertical" }, items: [{ type: "text", name: "v_only" }] },
];
const options: Options = { orientation: ["horizontal", "vertical"] };

describe("referencedFields", () => {
  it("collects name + value tokens + image.name, skips literal braces", () => {
    const f = referencedFields(layout, { orientation: "horizontal" });
    expect(f).toContain("title");
    expect(f).toContain("id");       // from {id} in the qr value
    expect(f).toContain("logo");     // image.name
    expect(f).toContain("h_only");   // matching container
    expect(f).not.toContain("v_only"); // gated out by option
    expect(f).not.toContain("not a field"); // {{ }} escape is literal
    expect(f).not.toContain("settings.qr_base_url"); // settings are not data fields
  });
  it("defaultOptions picks the first allowed value", () => {
    expect(defaultOptions(options)).toEqual({ orientation: "horizontal" });
  });
});

describe("imageFields", () => {
  it("returns data-bound image field names for the selection", () => {
    expect(imageFields(layout, { orientation: "horizontal" })).toEqual(["logo"]);
  });
});

describe("referencedSettings", () => {
  it("collects {settings.*} keys", () => {
    expect(referencedSettings(layout)).toContain("qr_base_url");
  });
});

describe("reconcileRowOptions", () => {
  const opts = { orientation: ["horizontal", "vertical"], outline: ["yes"] };
  it("defaults missing options to the first allowed value", () => {
    expect(reconcileRowOptions({}, opts)).toEqual({ orientation: "horizontal", outline: "yes" });
  });
  it("keeps an existing value for a still-declared option", () => {
    expect(reconcileRowOptions({ orientation: "vertical" }, opts)).toEqual({ orientation: "vertical", outline: "yes" });
  });
  it("drops options not declared by the template", () => {
    expect(reconcileRowOptions({ gone: "x", orientation: "vertical" }, opts)).toEqual({ orientation: "vertical", outline: "yes" });
  });
});

describe("tokens robustness", () => {
  it("does not throw on an unmatched brace", () => {
    const malformed: LayoutItem[] = [{ type: "text", value: "a{id" }];
    expect(() => referencedFields(malformed, {})).not.toThrow();
    expect(referencedFields(malformed, {})).not.toContain("id");
  });
});
