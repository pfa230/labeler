import { describe, it, expect, beforeEach } from "vitest";
import type { FieldSpec } from "../../api/connectors";
import { defaultColumnKeys, loadSavedColumnKeys, saveColumnKeys } from "./connectorColumns";

const mockColumns: FieldSpec[] = [
  { key: "name", label: "Name", ty: "text", tier: "cheap" },
  { key: "description", label: "Description", ty: "text", tier: "cheap" },
  { key: "manufacturer", label: "Manufacturer", ty: "text", tier: "hydrated" },
  { key: "item_url", label: "Homebox URL", ty: "text", tier: "derived" },
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

    it("returns all columns if no cheap tier columns exist", () => {
      const hydratedOnly: FieldSpec[] = [
        { key: "mfg", label: "Mfg", ty: "text", tier: "hydrated" },
      ];
      const keys = defaultColumnKeys(hydratedOnly);
      expect(Array.from(keys)).toEqual(["mfg"]);
    });
  });

  describe("loadSavedColumnKeys and saveColumnKeys", () => {
    it("returns default columns when storage is empty", () => {
      const keys = loadSavedColumnKeys("c1", "entities", mockColumns);
      expect(Array.from(keys)).toEqual(["name", "description"]);
    });

    it("persists and reloads saved column selections", () => {
      const selected = new Set(["name", "manufacturer"]);
      saveColumnKeys("c1", "entities", selected);
      const loaded = loadSavedColumnKeys("c1", "entities", mockColumns);
      expect(Array.from(loaded)).toEqual(["name", "manufacturer"]);
    });

    it("filters out obsolete/removed column keys from saved storage", () => {
      window.localStorage.setItem("labeler:connector-columns:c1:entities", JSON.stringify(["name", "removed_custom"]));
      const loaded = loadSavedColumnKeys("c1", "entities", mockColumns);
      expect(Array.from(loaded)).toEqual(["name"]);
    });

    it("falls back to defaults if stored keys are all invalid or empty array", () => {
      window.localStorage.setItem("labeler:connector-columns:c1:entities", JSON.stringify([]));
      const loaded = loadSavedColumnKeys("c1", "entities", mockColumns);
      expect(Array.from(loaded)).toEqual(["name", "description"]);
    });

    it("falls back to defaults on corrupt JSON in storage", () => {
      window.localStorage.setItem("labeler:connector-columns:c1:entities", "{bad json");
      const loaded = loadSavedColumnKeys("c1", "entities", mockColumns);
      expect(Array.from(loaded)).toEqual(["name", "description"]);
    });
  });
});
