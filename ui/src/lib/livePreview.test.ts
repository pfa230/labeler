import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { previewKey, useLivePreview, type PreviewInput } from "./livePreview";

const base: PreviewInput = { templateId: "t", format: "single", data: { x: "1" }, option: { o: "a" }, startSlot: 0 };

describe("previewKey", () => {
  it("is stable regardless of key insertion order, differs on data change", () => {
    const a = previewKey({ ...base, data: { x: "1", y: "2" } });
    const b = previewKey({ ...base, data: { y: "2", x: "1" } }); // reordered
    const c = previewKey({ ...base, data: { x: "9", y: "2" } });
    expect(a).toBe(b);
    expect(a).not.toBe(c);
  });
  it("omits an empty option object from the key", () => {
    expect(previewKey({ ...base, option: {} })).toBe(previewKey({ ...base, option: undefined }));
  });
});

describe("useLivePreview", () => {
  beforeEach(() => vi.stubGlobal("fetch", vi.fn(async () => new Response(new Blob(["x"]), { status: 200 }))));
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); }); // restore URL spies too

  it("does not fetch and reports not-loading when disabled", () => {
    const { result } = renderHook(() => useLivePreview(base, false, 0));
    expect(result.current.loading).toBe(false);
    expect(fetch).not.toHaveBeenCalled();
  });

  it("fetches once after the debounce and returns a url", async () => {
    const { result } = renderHook(() => useLivePreview(base, true, 0));
    await waitFor(() => expect(result.current.url).toBeDefined());
    expect(fetch).toHaveBeenCalledTimes(1);
    expect((fetch as ReturnType<typeof vi.fn>).mock.calls[0][0]).toBe("/api/render/label");
  });

  it("reuses the cache on re-render with the same input (no second fetch)", async () => {
    const { result, rerender } = renderHook((p: { i: PreviewInput }) => useLivePreview(p.i, true, 0), {
      initialProps: { i: base },
    });
    await waitFor(() => expect(result.current.url).toBeDefined());
    rerender({ i: { ...base } }); // equal key
    await waitFor(() => expect(result.current.url).toBeDefined());
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("posts /api/batch for a sheet and omits an empty option from the body", async () => {
    const sheet: PreviewInput = { templateId: "s", format: "sheet", data: { x: "1" }, option: {} };
    const { result } = renderHook(() => useLivePreview(sheet, true, 0));
    await waitFor(() => expect(result.current.url).toBeDefined());
    const [url, init] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(url).toBe("/api/batch");
    const body = JSON.parse((init as RequestInit).body as string);
    expect(body.labels[0].option).toBeUndefined(); // empty option omitted
    expect(body.mode).toBe("download");
  });

  it("revokes cached object URLs on unmount", async () => {
    const revoke = vi.spyOn(URL, "revokeObjectURL");
    const { result, unmount } = renderHook(() => useLivePreview(base, true, 0));
    await waitFor(() => expect(result.current.url).toBeDefined());
    unmount();
    expect(revoke).toHaveBeenCalled();
  });

  it("aborts the in-flight request on key change and does not let the stale response win", async () => {
    // First call HANGS until its AbortSignal fires (rejecting AbortError); second resolves immediately.
    let call = 0;
    vi.stubGlobal("fetch", vi.fn((_url: string, init: RequestInit) => {
      call += 1;
      if (call === 1) {
        return new Promise((_resolve, reject) => {
          init.signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
        });
      }
      return Promise.resolve(new Response(new Blob(["second"]), { status: 200 }));
    }));
    const { result, rerender } = renderHook((p: { i: PreviewInput }) => useLivePreview(p.i, true, 0), { initialProps: { i: base } });
    await waitFor(() => expect(call).toBe(1));            // ensure the first (hanging) fetch has STARTED
    rerender({ i: { ...base, data: { x: "2" } } });       // new key: cleanup aborts the first request
    await waitFor(() => expect(result.current.url).toBeDefined());
    expect(result.current.error).toBeUndefined();          // the aborted first request never sets an error or stale url
    expect(call).toBe(2);
  });
});
