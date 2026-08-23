import { describe, it, expect } from "vitest";
import { compareRowsBy } from "./connectorSort";
import type { DisplayRow, FieldSpec } from "../api/connectors";

function row(id: string, cells: DisplayRow["cells"]): DisplayRow {
  return { id: { resource: "items", key: id }, cells };
}

function field(key: string, ty: FieldSpec["ty"]): FieldSpec {
  return { key, label: key, ty, tier: "cheap" };
}

function sortedIds(rows: DisplayRow[], f: FieldSpec, direction: "asc" | "desc"): string[] {
  return [...rows].sort(compareRowsBy(f, direction)).map((r) => r.id.key);
}

describe("compareRowsBy", () => {
  describe("text", () => {
    const f = field("name", "text");

    it("orders case-insensitively, ascending", () => {
      const rows = [row("1", { name: "banana" }), row("2", { name: "Apple" }), row("3", { name: "cherry" })];
      expect(sortedIds(rows, f, "asc")).toEqual(["2", "1", "3"]);
    });

    it("orders case-insensitively, descending", () => {
      const rows = [row("1", { name: "banana" }), row("2", { name: "Apple" }), row("3", { name: "cherry" })];
      expect(sortedIds(rows, f, "desc")).toEqual(["3", "1", "2"]);
    });
  });

  describe("badge", () => {
    const f = field("status", "badge");

    it("orders case-insensitively, ascending", () => {
      const rows = [row("1", { status: "Warm" }), row("2", { status: "cold" }), row("3", { status: "Hot" })];
      expect(sortedIds(rows, f, "asc")).toEqual(["2", "3", "1"]);
    });

    it("orders case-insensitively, descending", () => {
      const rows = [row("1", { status: "Warm" }), row("2", { status: "cold" }), row("3", { status: "Hot" })];
      expect(sortedIds(rows, f, "desc")).toEqual(["1", "3", "2"]);
    });
  });

  describe("number", () => {
    const f = field("qty", "number");

    it("orders numerically, not lexicographically, ascending", () => {
      const rows = [row("1", { qty: 99.95 }), row("2", { qty: 2 }), row("3", { qty: 10 })];
      expect(sortedIds(rows, f, "asc")).toEqual(["2", "3", "1"]);
    });

    it("orders numerically, not lexicographically, descending", () => {
      const rows = [row("1", { qty: 99.95 }), row("2", { qty: 2 }), row("3", { qty: 10 })];
      expect(sortedIds(rows, f, "desc")).toEqual(["1", "3", "2"]);
    });

    it("accepts a numeric string as interpretable", () => {
      const rows = [row("1", { qty: "42" }), row("2", { qty: 7 })];
      expect(sortedIds(rows, f, "asc")).toEqual(["2", "1"]);
    });

    it("does not treat the empty string as the number zero", () => {
      const rows = [row("1", { qty: "" }), row("2", { qty: -5 })];
      // If "" were coerced via Number(""), it would read as 0 and sort before -5's... actually
      // after 0 but the key point is "" must land with the blanks (last), not as 0.
      expect(sortedIds(rows, f, "asc")).toEqual(["2", "1"]);
      expect(sortedIds(rows, f, "desc")).toEqual(["2", "1"]);
    });
  });

  describe("money", () => {
    const f = field("price", "money");

    it("orders numerically, not lexicographically, ascending", () => {
      const rows = [row("1", { price: 99.95 }), row("2", { price: 2 }), row("3", { price: 10 })];
      expect(sortedIds(rows, f, "asc")).toEqual(["2", "3", "1"]);
    });

    it("orders numerically, not lexicographically, descending", () => {
      const rows = [row("1", { price: 99.95 }), row("2", { price: 2 }), row("3", { price: 10 })];
      expect(sortedIds(rows, f, "desc")).toEqual(["1", "3", "2"]);
    });

    it("sorts a non-numeric money cell with the blanks, both directions, not as text", () => {
      const rows = [row("1", { price: "n/a" }), row("2", { price: 5 }), row("3", { price: 50 })];
      // Text order would put "n/a" before "5"/"50" (as strings "5" < "50" < "n/a" actually --
      // the key assertion is n/a lands LAST regardless of text-comparison outcome).
      expect(sortedIds(rows, f, "asc")).toEqual(["2", "3", "1"]);
      expect(sortedIds(rows, f, "desc")).toEqual(["3", "2", "1"]);
    });
  });

  describe("date", () => {
    const f = field("created", "date");

    it("orders chronologically over ISO-8601, ascending", () => {
      const rows = [
        row("1", { created: "2026-03-01" }),
        row("2", { created: "2024-01-15" }),
        row("3", { created: "2025-06-20" }),
      ];
      expect(sortedIds(rows, f, "asc")).toEqual(["2", "3", "1"]);
    });

    it("orders chronologically over ISO-8601, descending", () => {
      const rows = [
        row("1", { created: "2026-03-01" }),
        row("2", { created: "2024-01-15" }),
        row("3", { created: "2025-06-20" }),
      ];
      expect(sortedIds(rows, f, "desc")).toEqual(["1", "3", "2"]);
    });

    it("sorts an unparsable date string with the blanks, both directions", () => {
      const rows = [
        row("1", { created: "sometime in June" }),
        row("2", { created: "2025-06-20" }),
        row("3", { created: "2024-01-15" }),
      ];
      expect(sortedIds(rows, f, "asc")).toEqual(["3", "2", "1"]);
      expect(sortedIds(rows, f, "desc")).toEqual(["2", "3", "1"]);
    });

    it("rejects a non-ISO date format even though Date.parse alone would accept it", () => {
      // "06/20/2025" and "June 20, 2025" both parse successfully under plain Date.parse, but
      // neither matches the ISO-8601 shape the contract requires, so the module must reject them
      // as uninterpretable (last, with the blanks) rather than silently accepting loose formats.
      const rows = [
        row("1", { created: "06/20/2025" }),
        row("2", { created: "June 20, 2025" }),
        row("3", { created: "2025-06-20" }),
        row("4", { created: "2024-01-15" }),
      ];
      expect(sortedIds(rows, f, "asc")).toEqual(["4", "3", "1", "2"]);
      expect(sortedIds(rows, f, "desc")).toEqual(["3", "4", "1", "2"]);
    });

    it("does not accept a bare number as a date, even one that looks like a year", () => {
      const rows = [row("1", { created: 2026 }), row("2", { created: "2025-06-20" })];
      // A bare number is not ISO-shaped text, so it must be uninterpretable (last), not parsed
      // as a millisecond timestamp or compared as if 2026 were a valid moment.
      expect(sortedIds(rows, f, "asc")).toEqual(["2", "1"]);
      expect(sortedIds(rows, f, "desc")).toEqual(["2", "1"]);
    });
  });

  describe("blanks", () => {
    const f = field("name", "text");

    it("orders an absent key after every real value, ascending and descending", () => {
      const rows = [row("1", {}), row("2", { name: "banana" }), row("3", { name: "apple" })];
      expect(sortedIds(rows, f, "asc")).toEqual(["3", "2", "1"]);
      expect(sortedIds(rows, f, "desc")).toEqual(["2", "3", "1"]);
    });

    it("orders an empty-string cell after every real value, ascending and descending", () => {
      const rows = [row("1", { name: "" }), row("2", { name: "banana" }), row("3", { name: "apple" })];
      expect(sortedIds(rows, f, "asc")).toEqual(["3", "2", "1"]);
      expect(sortedIds(rows, f, "desc")).toEqual(["2", "3", "1"]);
    });

    it("keeps blanks last in descending order too, proving descending is not a plain reverse of ascending", () => {
      // A naive "reverse the ascending array" implementation would put blanks FIRST when
      // descending. The contract instead requires blanks to stay last in both directions, so the
      // asc and desc orderings of the non-blank rows are reverses of each other, but the blank's
      // position (always last) is not.
      const rows = [row("1", { name: "banana" }), row("2", {}), row("3", { name: "apple" })];
      const asc = sortedIds(rows, f, "asc");
      const desc = sortedIds(rows, f, "desc");
      expect(asc[asc.length - 1]).toBe("2");
      expect(desc[desc.length - 1]).toBe("2");
      expect(asc).not.toEqual([...desc].reverse());
    });
  });

  describe("tie stability", () => {
    it("keeps the connector's input order among rows with equal keys, ascending", () => {
      const f = field("status", "badge");
      const rows = [
        row("a", { status: "hot" }),
        row("b", { status: "hot" }),
        row("c", { status: "hot" }),
        row("d", { status: "hot" }),
      ];
      expect(sortedIds(rows, f, "asc")).toEqual(["a", "b", "c", "d"]);
    });

    it("keeps the connector's input order among rows with equal keys, descending", () => {
      const f = field("status", "badge");
      const rows = [
        row("a", { status: "hot" }),
        row("b", { status: "hot" }),
        row("c", { status: "hot" }),
        row("d", { status: "hot" }),
      ];
      expect(sortedIds(rows, f, "desc")).toEqual(["a", "b", "c", "d"]);
    });

    it("keeps the connector's input order among the trailing blank/uninterpretable group", () => {
      const f = field("qty", "number");
      const rows = [
        row("a", { qty: 5 }),
        row("b", {}),
        row("c", { qty: "n/a" }),
        row("d", { qty: "" }),
        row("e", { qty: 1 }),
      ];
      // Interpretable rows (a, e) sort by value; the blank/uninterpretable group (b, c, d) keeps
      // its relative input order, in both directions.
      expect(sortedIds(rows, f, "asc")).toEqual(["e", "a", "b", "c", "d"]);
      expect(sortedIds(rows, f, "desc")).toEqual(["a", "e", "b", "c", "d"]);
    });
  });
});
