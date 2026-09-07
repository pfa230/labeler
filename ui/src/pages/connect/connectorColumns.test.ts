import { describe, it, expect, beforeEach } from "vitest";
import type { FieldSpec } from "../../api/connectors";
import {
  defaultColumnKeys,
  isTransformDerived,
  loadSavedColumnChoice,
  makeColumnChoice,
  resolveColumnKeys,
  saveColumnChoice,
} from "./connectorColumns";

const mockColumns: FieldSpec[] = [
  { key: "name", label: "Name", ty: "text", tier: "cheap", multi_valued: false, transform_source: true },
  { key: "description", label: "Description", ty: "text", tier: "cheap", multi_valued: false, transform_source: true },
  { key: "manufacturer", label: "Manufacturer", ty: "text", tier: "hydrated", multi_valued: false, transform_source: true },
  { key: "item_url", label: "Homebox URL", ty: "text", tier: "derived", multi_valued: false, transform_source: true },
];

describe("connectorColumns helpers", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  describe("isTransformDerived", () => {
    it("identifies transform-derived columns by tier derived and not transform_source", () => {
      expect(isTransformDerived({ key: "k", label: "K", ty: "text", tier: "derived", multi_valued: false, transform_source: false })).toBe(true);
      expect(isTransformDerived({ key: "k", label: "K", ty: "text", tier: "derived", multi_valued: false, transform_source: true })).toBe(false);
      expect(isTransformDerived({ key: "k", label: "K", ty: "text", tier: "cheap", multi_valued: false, transform_source: false })).toBe(false);
      expect(isTransformDerived({ key: "k", label: "K", ty: "text", tier: "hydrated", multi_valued: false, transform_source: false })).toBe(false);
    });
  });

  describe("defaultColumnKeys", () => {
    it("returns cheap tier columns when available", () => {
      const keys = defaultColumnKeys(mockColumns);
      expect(Array.from(keys)).toEqual(["name", "description"]);
    });

    it("shows cheap columns plus transform-derived columns by default, keeping connector-derived hidden", () => {
      const columnsWithDerived: FieldSpec[] = [
        ...mockColumns,
        { key: "location_id", label: "Location ID", ty: "text", tier: "derived", multi_valued: false, transform_source: false },
      ];
      const keys = defaultColumnKeys(columnsWithDerived);
      expect(Array.from(keys)).toEqual(["name", "description", "location_id"]);
    });

    it("returns all columns if no cheap or transform-derived columns exist", () => {
      const neither: FieldSpec[] = [
        { key: "mfg", label: "Mfg", ty: "text", tier: "hydrated", multi_valued: false, transform_source: true },
        { key: "item_url", label: "Homebox URL", ty: "text", tier: "derived", multi_valued: false, transform_source: true },
      ];
      const keys = defaultColumnKeys(neither);
      expect(Array.from(keys)).toEqual(["mfg", "item_url"]);
    });
  });

  describe("makeColumnChoice", () => {
    it("records visible keys and tracks hidden transform-derived columns", () => {
      const columnsWithDerived: FieldSpec[] = [
        ...mockColumns,
        { key: "location_id", label: "Location ID", ty: "text", tier: "derived", multi_valued: false, transform_source: false },
      ];
      const choice = makeColumnChoice(columnsWithDerived, ["name"]);
      expect(choice).toEqual({
        visible: ["name"],
        hiddenDerived: ["location_id"],
      });
    });

    it("does not include visible transform-derived columns in hiddenDerived", () => {
      const columnsWithDerived: FieldSpec[] = [
        ...mockColumns,
        { key: "location_id", label: "Location ID", ty: "text", tier: "derived", multi_valued: false, transform_source: false },
      ];
      const choice = makeColumnChoice(columnsWithDerived, ["name", "location_id"]);
      expect(choice).toEqual({
        visible: ["name", "location_id"],
        hiddenDerived: [],
      });
    });

    it("accepts a Set of visible keys", () => {
      const choice = makeColumnChoice(mockColumns, new Set(["name", "manufacturer"]));
      expect(choice).toEqual({
        visible: ["name", "manufacturer"],
        hiddenDerived: [],
      });
    });
  });

  describe("loadSavedColumnChoice and saveColumnChoice", () => {
    it("returns null when storage is empty", () => {
      expect(loadSavedColumnChoice("c1", "entities")).toBeNull();
    });

    it("persists and reloads saved ColumnChoice", () => {
      const choice = { visible: ["name", "manufacturer"], hiddenDerived: ["location_id"] };
      saveColumnChoice("c1", "entities", choice);
      expect(loadSavedColumnChoice("c1", "entities")).toEqual(choice);
    });

    it("reads legacy array-shaped storage as visible keys with empty hiddenDerived", () => {
      window.localStorage.setItem(
        "labeler:connector-columns:c1:entities",
        JSON.stringify(["name", "description"])
      );
      expect(loadSavedColumnChoice("c1", "entities")).toEqual({
        visible: ["name", "description"],
        hiddenDerived: [],
      });
    });

    it("returns null on corrupt JSON in storage", () => {
      window.localStorage.setItem("labeler:connector-columns:c1:entities", "{bad json");
      expect(loadSavedColumnChoice("c1", "entities")).toBeNull();
    });
  });

  describe("resolveColumnKeys", () => {
    const columnsWithDerived: FieldSpec[] = [
      ...mockColumns,
      { key: "location_id", label: "Location ID", ty: "text", tier: "derived", multi_valued: false, transform_source: false },
    ];

    it("returns default column keys when choice is null or undefined", () => {
      expect(Array.from(resolveColumnKeys(mockColumns, null))).toEqual(["name", "description"]);
      expect(Array.from(resolveColumnKeys(columnsWithDerived, undefined))).toEqual(["name", "description", "location_id"]);
    });

    it("resolves visible regular columns from choice", () => {
      const choice = { visible: ["name", "manufacturer"], hiddenDerived: [] };
      const resolved = resolveColumnKeys(mockColumns, choice);
      expect(Array.from(resolved)).toEqual(["name", "manufacturer"]);
    });

    it("filters out obsolete/removed column keys from choice", () => {
      const choice = { visible: ["name", "removed_custom"], hiddenDerived: [] };
      const resolved = resolveColumnKeys(mockColumns, choice);
      expect(Array.from(resolved)).toEqual(["name"]);
    });

    it("falls back to defaults if stored keys are all invalid or empty array", () => {
      const choice = { visible: [], hiddenDerived: [] };
      const resolved = resolveColumnKeys(mockColumns, choice);
      expect(Array.from(resolved)).toEqual(["name", "description"]);
    });

    it("stored choice hides what it hid and still shows a transform-derived column it never saw", () => {
      // Stored choice made before location_id existed (only hid description, chose name)
      const choice = { visible: ["name"], hiddenDerived: [] };
      const resolved = resolveColumnKeys(columnsWithDerived, choice);
      expect(Array.from(resolved)).toEqual(["name", "location_id"]);
    });

    it("hidden transform-derived column is kept hidden by hiddenDerived list", () => {
      // Choice explicitly hid location_id
      const choice = { visible: ["name"], hiddenDerived: ["location_id"] };
      const resolved = resolveColumnKeys(columnsWithDerived, choice);
      expect(Array.from(resolved)).toEqual(["name"]);
    });

    it("preserves definition order of columns when resolving", () => {
      // Visible keys in different order in choice
      const choice = { visible: ["manufacturer", "name"], hiddenDerived: [] };
      const resolved = resolveColumnKeys(mockColumns, choice);
      expect(Array.from(resolved)).toEqual(["name", "manufacturer"]);
    });

    it("end-to-end: reloads choice from legacy storage and resolves showing new transform-derived column", () => {
      window.localStorage.setItem(
        "labeler:connector-columns:c1:entities",
        JSON.stringify(["name"])
      );
      const choice = loadSavedColumnChoice("c1", "entities");
      const resolved = resolveColumnKeys(columnsWithDerived, choice);
      // "name" is visible, "description" (cheap) is hidden, "item_url" (connector-derived) is hidden,
      // "location_id" (transform-derived) is shown
      expect(Array.from(resolved)).toEqual(["name", "location_id"]);
    });

    it("end-to-end: saves choice via makeColumnChoice + saveColumnChoice and resolves on reload", () => {
      // User hid location_id and description, selected only name
      const choice = makeColumnChoice(columnsWithDerived, new Set(["name"]));
      saveColumnChoice("c1", "entities", choice);

      const loadedChoice = loadSavedColumnChoice("c1", "entities");
      const resolved = resolveColumnKeys(columnsWithDerived, loadedChoice);
      expect(Array.from(resolved)).toEqual(["name"]);
    });
  });
});

