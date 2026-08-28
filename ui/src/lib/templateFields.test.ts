import { describe, it, expect } from "vitest";
import {
  reconcileRowOptions,
  hasServerDefault,
  datetimeCellError,
  formatLocalDate,
  formatLocalDateTime,
  isLeapYear,
  daysInMonth,
} from "./templateFields";

describe("hasServerDefault", () => {
  it("returns true for datetime, boolean, enum/select with values, and spec with default", () => {
    expect(hasServerDefault({ type: "datetime" })).toBe(true);
    expect(hasServerDefault({ control: "datetime" })).toBe(true);
    expect(hasServerDefault({ type: "boolean" })).toBe(true);
    expect(hasServerDefault({ control: "checkbox" })).toBe(true);
    expect(hasServerDefault({ type: "enum", values: ["a", "b"] })).toBe(true);
    expect(hasServerDefault({ control: "select", values: ["a", "b"] })).toBe(true);
    expect(hasServerDefault({ type: "string", default: "hello" })).toBe(true);
    expect(hasServerDefault({ type: "string" })).toBe(false);
    expect(hasServerDefault({ type: "integer" })).toBe(false);
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

describe("date formatters", () => {
  it("formats local date and datetime", () => {
    const d = new Date(2026, 7, 19, 14, 30, 0);
    expect(formatLocalDate(d)).toBe("2026-08-19");
    expect(formatLocalDateTime(d)).toBe("2026-08-19T14:30");
  });

  it("calculates leap years and days in month", () => {
    expect(isLeapYear(2024)).toBe(true);
    expect(isLeapYear(2026)).toBe(false);
    expect(isLeapYear(2000)).toBe(true);
    expect(isLeapYear(1900)).toBe(false);
    expect(daysInMonth(2024, 2)).toBe(29);
    expect(daysInMonth(2026, 2)).toBe(28);
    expect(daysInMonth(2026, 4)).toBe(30);
    expect(daysInMonth(2026, 1)).toBe(31);
  });
});
