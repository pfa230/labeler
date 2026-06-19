import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { useRowPreview } from "./rowPreview";

beforeEach(() => {
  vi.stubGlobal("URL", { createObjectURL: () => "blob:x", revokeObjectURL: vi.fn() } as unknown as typeof URL);
});

describe("useRowPreview", () => {
  it("renders the selected single row via /render/label", async () => {
    const fetchMock = vi.fn(async () => new Response(new Blob([new Uint8Array([1])]), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const { result } = renderHook(() =>
      useRowPreview({ templateId: "t", format: "single", label: { data: { title: "x" } } }),
    );
    await waitFor(() => expect(result.current.url).toBe("blob:x"));
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/render/label",
      expect.objectContaining({ method: "POST" }),
    );
  });

  it("is idle with no selected label", () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    const { result } = renderHook(() => useRowPreview({ templateId: "t", format: "single" }));
    expect(result.current.loading).toBe(false);
    expect(result.current.url).toBeUndefined();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
