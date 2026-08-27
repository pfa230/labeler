import { describe, it, expect } from "vitest";
import {
  referencedFields,
  referencedVariables,
  defaultOptions,
  imageFields,
  multilineFields,
  singleLineTextFields,
  reconcileRowOptions,
  hasServerDefault,
  datetimeCellError,
  initialParamValues,
} from "./templateFields";
import type { LayoutItem, Options } from "../api/types";

const layout: LayoutItem[] = [
  { type: "text", value: "{title}" },
  { type: "qr", value: "{vars.qr_base_url}/{id}" },
  { type: "image", name: "logo" },
  { type: "text", value: "literal {{not a field}}" },
  { type: "container", option: { orientation: "horizontal" }, items: [{ type: "text", value: "{h_only}" }] },
  { type: "container", option: { orientation: "vertical" }, items: [{ type: "text", value: "{v_only}" }] },
];
const options: Options = { orientation: ["horizontal", "vertical"] };

describe("referencedFields", () => {
  it("collects value tokens + image.name, skips literal braces", () => {
    const f = referencedFields(layout, { orientation: "horizontal" });
    expect(f).toContain("title");
    expect(f).toContain("id");       // from {id} in the qr value
    expect(f).toContain("logo");     // image.name
    expect(f).toContain("h_only");   // matching container
    expect(f).not.toContain("v_only"); // gated out by option
    expect(f).not.toContain("not a field"); // {{ }} escape is literal
    expect(f).not.toContain("vars.qr_base_url"); // variables are not data fields
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

describe("multilineFields / singleLineTextFields", () => {
  it("classifies fields by whether their text item is multiline", () => {
    const layout: LayoutItem[] = [
      { type: "text", value: "{body}", multiline: true },
      { type: "text", value: "{title}" },
      { type: "qr", value: "{code}" },
      { type: "image", name: "logo" },
    ];
    expect(multilineFields(layout, {})).toEqual(["body"]);
    expect(singleLineTextFields(layout, {})).toEqual(["title"]);
  });

  it("excludes vars and sys tokens from both walks", () => {
    const layout: LayoutItem[] = [
      { type: "text", value: "{a} {vars.b} {sys.now} {sys.now:short}", multiline: true },
      { type: "text", value: "{c} {vars.d}" },
    ];
    expect(multilineFields(layout, {})).toEqual(["a"]);
    expect(singleLineTextFields(layout, {})).toEqual(["c"]);
  });

  /// The control follows the branch on screen…
  it("gates multilineFields on the selected option", () => {
    const layout: LayoutItem[] = [
      {
        type: "container",
        option: { mode: "long" },
        items: [{ type: "text", value: "{body}", multiline: true }],
      },
    ];
    expect(multilineFields(layout, { mode: "long" })).toEqual(["body"]);
    expect(multilineFields(layout, { mode: "short" })).toEqual([]);
  });

  /// …but an empty selection means "every branch", which is how the warning is computed. If someone
  /// gates the warning, this is the test that fails (spec Decision 1).
  it("walks every branch when the selection is empty", () => {
    const layout: LayoutItem[] = [
      {
        type: "container",
        option: { mode: "long" },
        items: [{ type: "text", value: "{shared}", multiline: true }],
      },
      {
        type: "container",
        option: { mode: "short" },
        items: [{ type: "text", value: "{shared}" }],
      },
    ];
    expect(multilineFields(layout, {})).toEqual(["shared"]);
    expect(singleLineTextFields(layout, {})).toEqual(["shared"]);
  });

  it("retains declared non-datetime parameters in multilineFields and singleLineTextFields", () => {
    const layout: LayoutItem[] = [
      {
        type: "container",
        option: { mode: "long" },
        items: [{ type: "text", value: "{notes}", multiline: true }],
      },
      {
        type: "container",
        option: { mode: "short" },
        items: [{ type: "text", value: "{notes}" }],
      },
    ];
    const params = {
      notes: { type: "string" as const },
    };
    expect(multilineFields(layout, {}, params)).toEqual(["notes"]);
    expect(singleLineTextFields(layout, {}, params)).toEqual(["notes"]);
  });
});

describe("referencedVariables", () => {
  it("collects {vars.*} keys", () => {
    expect(referencedVariables(layout)).toContain("qr_base_url");
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
  it("keeps a present-but-blank value (so it stays invalid), only defaults absent options", () => {
    expect(reconcileRowOptions({ orientation: "" }, opts)).toEqual({ orientation: "", outline: "yes" });
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

describe("referencedFields token grammar", () => {
  it("treats bare datetime as data field, excludes sys.now and sys.now:<fmt>", () => {
    const items: LayoutItem[] = [
      { type: "text", value: "{sys.now:short_date} {sys.now} {datetime} {title:short_date}" },
      { type: "text", value: "{datetimefoo}" },
      { type: "text", value: "{product_id}" },
    ];
    const f = referencedFields(items, {});
    expect(f).toContain("datetime");
    expect(f).toContain("title");
    expect(f).toContain("datetimefoo");
    expect(f).toContain("product_id");
    expect(f).not.toContain("sys.now");
    expect(f).not.toContain("sys.now:short_date");
    expect(f).not.toContain("short_date");
  });

  it("excludes declared parameter namespaces ({p} and {p:<fmt>})", () => {
    const items: LayoutItem[] = [
      { type: "text", value: "{printed_on} {printed_on:short_date} {title}" },
    ];
    const params = {
      printed_on: { type: "datetime" as const },
    };
    const f = referencedFields(items, {}, params);
    expect(f).toEqual(["title"]);
    expect(f).not.toContain("printed_on");
    expect(f).not.toContain("printed_on:short_date");
  });
});

describe("hasServerDefault", () => {
  it("returns true for datetime, boolean, enum with values, and spec with default", () => {
    expect(hasServerDefault({ type: "datetime" })).toBe(true);
    expect(hasServerDefault({ type: "datetime", time: true })).toBe(true);
    expect(hasServerDefault({ type: "boolean" })).toBe(true);
    expect(hasServerDefault({ type: "enum", values: ["a", "b"] })).toBe(true);
    expect(hasServerDefault({ type: "string", default: "hello" })).toBe(true);
    expect(hasServerDefault({ type: "string" })).toBe(false);
    expect(hasServerDefault({ type: "integer" })).toBe(false);
  });
});

describe("datetimeCellError", () => {
  it("accepts blank values", () => {
    expect(datetimeCellError("")).toBeNull();
    expect(datetimeCellError("   ")).toBeNull();
  });

  it("accepts valid date and date-time formats", () => {
    expect(datetimeCellError("2026-08-19")).toBeNull();
    expect(datetimeCellError("2026-08-19T14:30")).toBeNull();
    expect(datetimeCellError("2026-08-19T14:30:45")).toBeNull();
    expect(datetimeCellError("2026-08-19T14:30:00Z")).toBeNull();
    expect(datetimeCellError("2026-08-19T14:30:00+02:00")).toBeNull();
    expect(datetimeCellError("2026-08-19T14:30:00-05:00")).toBeNull();
  });

  it("rejects non-matching formats", () => {
    expect(datetimeCellError("yesterday")).not.toBeNull();
    expect(datetimeCellError("08/19/2026")).not.toBeNull();
    expect(datetimeCellError("2026-8-19")).not.toBeNull();
  });

  it("rejects invalid calendar values", () => {
    expect(datetimeCellError("2026-02-29")).not.toBeNull(); // 2026 is not a leap year
    expect(datetimeCellError("2024-02-29")).toBeNull();    // 2024 is a leap year
    expect(datetimeCellError("2026-04-31")).not.toBeNull(); // April has 30 days
    expect(datetimeCellError("2026-13-01")).not.toBeNull(); // Month 13
    expect(datetimeCellError("2026-08-19T25:00")).not.toBeNull(); // Hour 25
    expect(datetimeCellError("2026-08-19T14:60")).not.toBeNull(); // Min 60
  });
});

describe("initialParamValues", () => {
  it("seeds browser-local date/time strings for datetime params", () => {
    const fixedNow = new Date(2026, 7, 19, 14, 30, 0); // month 7 is August (0-indexed)
    const params = {
      p_date: { type: "datetime" as const },
      p_datetime: { type: "datetime" as const, time: true },
      p_str: { type: "string" as const, default: "foo" },
      p_bool: { type: "boolean" as const },
    };
    const values = initialParamValues(params, fixedNow);
    expect(values.p_date).toBe("2026-08-19");
    expect(values.p_datetime).toBe("2026-08-19T14:30");
    expect(values.p_str).toBe("foo");
    expect(values.p_bool).toBe(false);
  });
});
