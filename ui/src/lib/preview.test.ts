import { describe, it, expect, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { sampleData, useTemplatePreview } from "./preview";
import * as client from "../api/client";
import type { TemplateDetail } from "../api/types";

describe("sampleData", () => {
  it("builds a value per referenced field with thumbnail rule", () => {
    const fixedNow = new Date("2026-08-29T12:34:56.000Z");
    expect(
      sampleData(
        [
          { name: "title", control: "text", interpolated: true, required: true },
          { name: "id", control: "number", interpolated: true, required: true },
          { name: "flavor", control: "select", values: ["vanilla", "chocolate"] },
          { name: "active", control: "checkbox", interpolated: true, required: true },
          { name: "printed_on", control: "datetime", interpolated: true, required: true },
        ],
        fixedNow,
      ),
    ).toEqual({
      title: "title",
      id: 1,
      flavor: "vanilla",
      active: false,
      printed_on: "2026-08-29T12:34:56.000Z",
    });
  });

  it("generates full timestamp with hours/minutes so preview and thumbnail agree on {printed_on:time}", () => {
    const fixedNow = new Date("2026-08-29T15:45:30.000Z");
    const data = sampleData(
      [{ name: "printed_on", control: "datetime", interpolated: true, required: true }],
      fixedNow,
    );
    expect(data.printed_on).toBe("2026-08-29T15:45:30.000Z");
    expect(String(data.printed_on)).toContain("15:45:30");
  });

  it("selects first enum value for undefaulted enum in inputs.all", () => {
    const data = sampleData([
      { name: "mode", control: "select", values: ["draft", "final"], required: true },
    ]);
    expect(data.mode).toBe("draft");
  });

  it("emits non-empty array for required interpolated list in inputs.all", () => {
    const data = sampleData([
      { name: "tags", control: "list", interpolated: true, required: true },
    ]);
    expect(data.tags).toEqual(["tags"]);
  });
});

describe("useTemplatePreview", () => {
  it("reports loading (not the idle empty-state) before a detail-driven render resolves (#74)", () => {
    const { result } = renderHook(() => useTemplatePreview(undefined));
    expect(result.current.loading).toBe(true);
    expect(result.current.url).toBeUndefined();
    expect(result.current.error).toBeUndefined();
  });

  it("renders preview with sample data for undefaulted datetime and enum parameters", async () => {
    const fetchBlobSpy = vi.spyOn(client, "fetchBlob").mockResolvedValue({
      blob: new Blob(["fake-png"], { type: "image/png" }),
    });

    const detail: TemplateDetail = {
      params: [],
      id: "tpl_dt_enum",
      name: "DT and Enum",
      unit: "mm",
      dpi: 200,
      format: { type: "single", width: 50, height: 20 },
      description: "",
      variables: [],
      inputs: {
        all: [
          { name: "printed_on", control: "datetime", interpolated: true, required: true },
          { name: "style", control: "select", values: ["fancy", "plain"], required: true },
        ],
        default: [
          { name: "printed_on", control: "datetime", interpolated: true, required: true },
          { name: "style", control: "select", values: ["fancy", "plain"], required: true },
        ],
      },
    };

    const { result } = renderHook(() => useTemplatePreview(detail));

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(fetchBlobSpy).toHaveBeenCalledWith(
      "/render/label",
      expect.objectContaining({
        method: "POST",
        body: expect.stringMatching(/"printed_on":".*T.*Z".*"style":"fancy"/),
      }),
    );
    expect(result.current.url).toBeDefined();

    fetchBlobSpy.mockRestore();
  });
});
