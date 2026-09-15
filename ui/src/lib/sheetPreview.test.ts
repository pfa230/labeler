import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useSheetPreview } from "./sheetPreview";

const revokeObjectURL = vi.fn();

beforeEach(() => {
  revokeObjectURL.mockReset();
  vi.stubGlobal("URL", {
    createObjectURL: () => "blob:x",
    revokeObjectURL,
  } as unknown as typeof URL);
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("useSheetPreview", () => {
  it("with fake timers, three rerenders inside the debounce produce one fetch carrying the last batch", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn<typeof fetch>(async () => new Response(new Blob([new Uint8Array([1])]), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    const { rerender } = renderHook(
      (input) => useSheetPreview(input, true),
      {
        initialProps: {
          templateId: "t",
          labels: [{ data: { a: "1" } }],
        },
      },
    );

    act(() => {
      vi.advanceTimersByTime(100);
    });

    rerender({
      templateId: "t",
      labels: [{ data: { a: "2" } }],
    });

    act(() => {
      vi.advanceTimersByTime(100);
    });

    rerender({
      templateId: "t",
      labels: [{ data: { a: "3" } }],
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const body = JSON.parse(String(fetchMock.mock.calls[0][1]?.body));
    expect(body.labels).toEqual([{ data: { a: "3" } }]);
  });

  it("a fetch that never resolves has its signal.aborted set by a rerender with a new key and the hook reports neither a URL nor an error for it", async () => {
    vi.useFakeTimers();
    let capturedSignal: AbortSignal | undefined;
    const pendingPromise = new Promise<Response>(() => {});
    const fetchMock = vi.fn((_url, init) => {
      capturedSignal = init?.signal;
      return pendingPromise;
    });
    vi.stubGlobal("fetch", fetchMock);

    const { result, rerender } = renderHook(
      (input) => useSheetPreview(input, true),
      {
        initialProps: {
          templateId: "t",
          labels: [{ data: { a: "1" } }],
        },
      },
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(capturedSignal?.aborted).toBe(false);

    rerender({
      templateId: "t",
      labels: [{ data: { a: "2" } }],
    });

    expect(capturedSignal?.aborted).toBe(true);
    expect(result.current.url).toBeUndefined();
    expect(result.current.error).toBeUndefined();
    expect(result.current.loading).toBe(true);
  });

  it("enabled: false sends nothing and reports not loading", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);

    const { result } = renderHook(() =>
      useSheetPreview({ templateId: "t", labels: [{ data: { a: "1" } }] }, false),
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(500);
    });

    expect(fetchMock).not.toHaveBeenCalled();
    expect(result.current.loading).toBe(false);
    expect(result.current.url).toBeUndefined();
    expect(result.current.error).toBeUndefined();
  });

  it("the body carries every label in order and start_slot", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn<typeof fetch>(async () => new Response(new Blob([new Uint8Array([1])]), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    renderHook(() =>
      useSheetPreview(
        {
          templateId: "t1",
          labels: [{ data: { sku: "1" } }, { data: { sku: "2" } }],
          startSlot: 4,
        },
        true,
      ),
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/batch",
      expect.objectContaining({
        method: "POST",
        headers: { "content-type": "application/json" },
      }),
    );
    const body = JSON.parse(String(fetchMock.mock.calls[0][1]?.body));
    expect(body).toEqual({
      template: "t1",
      mode: "download",
      labels: [{ data: { sku: "1" } }, { data: { sku: "2" } }],
      start_slot: 4,
    });
  });

  it("an equal key on rerender sends no second request", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn(async () => new Response(new Blob([new Uint8Array([1])]), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    const { rerender } = renderHook(
      (input) => useSheetPreview(input, true),
      {
        initialProps: {
          templateId: "t",
          labels: [{ data: { a: "1", b: "2" } }],
          startSlot: 1,
        },
      },
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);

    // Rerender with equal key (different object references, inverted keys)
    rerender({
      templateId: "t",
      labels: [{ data: { b: "2", a: "1" } }],
      startSlot: 1,
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(500);
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("unmount revokes the URL", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn(async () => new Response(new Blob([new Uint8Array([1])]), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    const { result, unmount } = renderHook(() =>
      useSheetPreview({ templateId: "t", labels: [{ data: { a: "1" } }] }, true),
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    expect(result.current.url).toBe("blob:x");
    revokeObjectURL.mockReset();
    unmount();
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:x");
  });
});
