import { describe, it, expect, beforeEach } from "vitest";
import type { FieldSpec } from "../../api/connectors";
import {
  defaultColumnKeys,
  loadSavedColumnChoice,
  makeColumnChoice,
  resolveColumnKeys,
  saveColumnChoice,
} from "./connectorColumns";

const mockColumns: FieldSpec[] = [
  { key: "name", label: "Name", ty: "text", tier: "cheap", multi_valued: false },
  { key: "description", label: "Description", ty: "text", tier: "cheap", multi_valued: false },
  { key: "manufacturer", label: "Manufacturer", ty: "text", tier: "hydrated", multi_valued: false },
  { key: "item_url", label: "Homebox URL", ty: "text", tier: "derived", multi_valued: false },
];

describe("connectorColumns helpers", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  describe("defaultColumnKeys", () => {
    it("returns cheap tier columns when available", () => {
      const keys = defaultColumnKeys(mockColumns);
      expect(Array.from(keys)).toEqual(["name", "description"]);
    });

    it("returns all columns if no cheap columns exist", () => {
      const noCheap: FieldSpec[] = [
        { key: "mfg", label: "Mfg", ty: "text", tier: "hydrated", multi_valued: false },
        { key: "item_url", label: "Homebox URL", ty: "text", tier: "derived", multi_valued: false },
      ];
      const keys = defaultColumnKeys(noCheap);
      expect(Array.from(keys)).toEqual(["mfg", "item_url"]);
    });
  });

  describe("makeColumnChoice", () => {
    it("records the visible keys from a Set", () => {
      const choice = makeColumnChoice(new Set(["name", "manufacturer"]));
      expect(choice).toEqual({ visible: ["name", "manufacturer"] });
    });
  });

  describe("loadSavedColumnChoice and saveColumnChoice", () => {
    it("returns null when storage is empty", () => {
      expect(loadSavedColumnChoice("c1", "entities")).toBeNull();
    });

    it("persists and reloads saved ColumnChoice", () => {
      const choice = { visible: ["name", "manufacturer"] };
      saveColumnChoice("c1", "entities", choice);
      expect(loadSavedColumnChoice("c1", "entities")).toEqual(choice);
    });

    it("reads a stored plain array as no choice", () => {
      window.localStorage.setItem(
        "labeler:connector-columns:c1:entities",
        JSON.stringify(["name", "description"])
      );
      expect(loadSavedColumnChoice("c1", "entities")).toBeNull();
    });

    it("reads the visible keys of a stored value that also carries hiddenDerived", () => {
      window.localStorage.setItem(
        "labeler:connector-columns:c1:entities",
        JSON.stringify({ visible: ["name"], hiddenDerived: ["location_id"] })
      );
      expect(loadSavedColumnChoice("c1", "entities")).toEqual({ visible: ["name"] });
    });

    it("returns null on corrupt JSON in storage", () => {
      window.localStorage.setItem("labeler:connector-columns:c1:entities", "{bad json");
      expect(loadSavedColumnChoice("c1", "entities")).toBeNull();
    });
  });

  describe("resolveColumnKeys", () => {
    it("returns default column keys when choice is null or undefined", () => {
      expect(Array.from(resolveColumnKeys(mockColumns, null))).toEqual(["name", "description"]);
      expect(Array.from(resolveColumnKeys(mockColumns, undefined))).toEqual(["name", "description"]);
    });

    it("resolves visible columns from choice", () => {
      const resolved = resolveColumnKeys(mockColumns, { visible: ["name", "manufacturer"] });
      expect(Array.from(resolved)).toEqual(["name", "manufacturer"]);
    });

    it("filters out obsolete/removed column keys from choice", () => {
      const resolved = resolveColumnKeys(mockColumns, { visible: ["name", "removed_custom"] });
      expect(Array.from(resolved)).toEqual(["name"]);
    });

    it("falls back to defaults if stored keys are all invalid or empty array", () => {
      const resolved = resolveColumnKeys(mockColumns, { visible: [] });
      expect(Array.from(resolved)).toEqual(["name", "description"]);
    });

    it("preserves definition order of columns when resolving", () => {
      const resolved = resolveColumnKeys(mockColumns, { visible: ["manufacturer", "name"] });
      expect(Array.from(resolved)).toEqual(["name", "manufacturer"]);
    });

    it("end-to-end: saves choice via makeColumnChoice + saveColumnChoice and resolves on reload", () => {
      saveColumnChoice("c1", "entities", makeColumnChoice(new Set(["name"])));
      const resolved = resolveColumnKeys(mockColumns, loadSavedColumnChoice("c1", "entities"));
      expect(Array.from(resolved)).toEqual(["name"]);
    });
  });
});
