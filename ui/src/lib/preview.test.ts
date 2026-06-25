import { describe, it, expect } from "vitest";
import { renderHook } from "@testing-library/react";
import { sampleData, useTemplatePreview } from "./preview";

describe("sampleData", () => {
  it("builds a value per referenced field", () => {
    expect(sampleData(["title", "id"])).toEqual({ title: "title", id: "id" });
  });
});

describe("useTemplatePreview", () => {
  it("reports loading (not the idle empty-state) before a detail-driven render resolves (#74)", () => {
    // With an undefined detail the effect returns early without flipping state, so this reflects the
    // hook's initial value. It must be `loading`: TemplateDetail always auto-previews, so the pane must
    // never flash PreviewPane's "Fill the required fields to preview." empty-state copy.
    const { result } = renderHook(() => useTemplatePreview(undefined));
    expect(result.current.loading).toBe(true);
    expect(result.current.url).toBeUndefined();
    expect(result.current.error).toBeUndefined();
  });
});
