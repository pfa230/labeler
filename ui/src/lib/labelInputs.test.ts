import { describe, it, expect, vi, afterEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { labelInputsKey, useLabelInputs, pruneDataForSubmit } from "./labelInputs";
import type { InputSpec } from "../api/types";

describe("labelInputsKey", () => {
  it("sorts keys alphabetically with templateId", () => {
    expect(labelInputsKey("t1", { b: 2, a: 1 })).toBe(labelInputsKey("t1", { a: 1, b: 2 }));
    expect(labelInputsKey("t1", { z: "last", a: "first" })).toBe('["t1",{"a":"first","z":"last"}]');
  });

  it("handles empty data", () => {
    expect(labelInputsKey("t1", {})).toBe('["t1",{}]');
  });
});

describe("pruneDataForSubmit", () => {
  it("omits names absent from activeInputs even when data has a non-empty value", () => {
    const activeInputs: InputSpec[] = [
      { name: "title", control: "text" },
      { name: "orientation", control: "select", values: ["horizontal", "vertical"] },
    ];
    const data = {
      title: "My Label",
      orientation: "horizontal",
      deactivated_field: "old value",
      other_inactive: 123,
    };
    const pruned = pruneDataForSubmit(data, activeInputs);
    expect(pruned).toEqual({
      title: "My Label",
      orientation: "horizontal",
    });
    expect(pruned).not.toHaveProperty("deactivated_field");
    expect(pruned).not.toHaveProperty("other_inactive");
  });

  it("submits empty strings for text, textarea, and image controls", () => {
    const activeInputs: InputSpec[] = [
      { name: "t", control: "text" },
      { name: "ta", control: "textarea" },
      { name: "img", control: "image" },
    ];
    const data = { t: "", ta: "", img: "" };
    expect(pruneDataForSubmit(data, activeInputs)).toEqual({
      t: "",
      ta: "",
      img: "",
    });
  });

  it("omits empty strings for non-text controls (integer, number, select, checkbox, datetime)", () => {
    const activeInputs: InputSpec[] = [
      { name: "count", control: "integer" },
      { name: "price", control: "number" },
      { name: "tier", control: "select", values: ["a", "b"] },
      { name: "flag", control: "checkbox" },
      { name: "day", control: "date" },
      { name: "printed_on", control: "datetime" },
    ];
    const data = {
      count: "",
      price: "",
      tier: "",
      flag: "",
      day: "",
      printed_on: "",
    };
    expect(pruneDataForSubmit(data, activeInputs)).toEqual({});
  });

  it("omits deferred names from the result while retaining non-deferred names", () => {
    const activeInputs: InputSpec[] = [
      { name: "title", control: "text", default: "Untitled" },
      { name: "count", control: "integer", default: 1 },
      { name: "notes", control: "text" },
    ];
    const data = {
      title: "Untitled",
      count: 1,
      notes: "Custom note",
    };
    const deferred = {
      title: true,
      count: false,
    };
    const pruned = pruneDataForSubmit(data, activeInputs, deferred);
    expect(pruned).toEqual({
      count: 1,
      notes: "Custom note",
    });
    expect(pruned).not.toHaveProperty("title");
  });
});

describe("useLabelInputs", () => {
  const defaultInputs: InputSpec[] = [
    { name: "title", control: "text" },
    { name: "tier", control: "select", values: ["standard", "pro"] },
  ];

  const gatedInputs: InputSpec[] = [
    { name: "title", control: "text" },
    { name: "tier", control: "select", values: ["standard", "pro"] },
    { name: "pro_code", control: "text" },
  ];

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("fetches inputs after debounce and completes", async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ inputs: [defaultInputs] }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const { result } = renderHook(() =>
      useLabelInputs("tmpl-1", () => ({ tier: "standard" }), defaultInputs, 20),
    );

    expect(result.current.inputs).toEqual(defaultInputs);

    await waitFor(() => expect(result.current.pending).toBe(false));
    expect(fetchMock).toHaveBeenCalled();
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("/api/templates/tmpl-1/inputs");
    expect(JSON.parse(init.body as string)).toEqual({
      labels: [{ data: { tier: "standard" } }],
    });
    expect(result.current.inputs).toEqual(defaultInputs);
  });

  it("holds previous list while pending during an update", async () => {
    let resolveSecond: (v: Response) => void;
    const secondPromise = new Promise<Response>((res) => { resolveSecond = res; });

    let call = 0;
    const fetchMock = vi.fn(async () => {
      call += 1;
      if (call === 1) {
        return new Response(JSON.stringify({ inputs: [defaultInputs] }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return secondPromise;
    });
    vi.stubGlobal("fetch", fetchMock);

    const { result, rerender } = renderHook(
      ({ data }) => useLabelInputs("tmpl-2", () => data, defaultInputs, 20),
      { initialProps: { data: { tier: "standard" } } },
    );

    await waitFor(() => expect(result.current.pending).toBe(false));
    expect(result.current.inputs).toEqual(defaultInputs);

    // Trigger update to tier: "pro"
    rerender({ data: { tier: "pro" } });

    // While pending, previous inputs should still be held
    expect(result.current.pending).toBe(true);
    expect(result.current.inputs).toEqual(defaultInputs);

    // Resolve second request
    resolveSecond!(
      new Response(JSON.stringify({ inputs: [gatedInputs] }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );

    await waitFor(() => expect(result.current.pending).toBe(false));
    expect(result.current.inputs).toEqual(gatedInputs);
  });

  it("serves repeated queries from cache without calling API", async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ inputs: [gatedInputs] }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const { result, rerender } = renderHook(
      ({ data }) => useLabelInputs("tmpl-cache", () => data, defaultInputs, 20),
      { initialProps: { data: { tier: "pro" } } },
    );

    await waitFor(() => expect(result.current.pending).toBe(false));
    expect(fetchMock).toHaveBeenCalledTimes(1);

    // Switch to another data
    rerender({ data: { tier: "standard" } });
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));

    // Switch back to tier: "pro" -> should hit cache immediately without fetch call 3
    rerender({ data: { tier: "pro" } });
    expect(result.current.inputs).toEqual(gatedInputs);
    await waitFor(() => expect(result.current.pending).toBe(false));
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("aborts in-flight request on supersede", async () => {
    let call = 0;
    const fetchMock = vi.fn((_url: string, init: RequestInit) => {
      call += 1;
      if (call === 1) {
        return new Promise((_resolve, reject) => {
          init.signal?.addEventListener("abort", () =>
            reject(new DOMException("aborted", "AbortError")),
          );
        });
      }
      return Promise.resolve(
        new Response(JSON.stringify({ inputs: [gatedInputs] }), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
      );
    });
    vi.stubGlobal("fetch", fetchMock);

    const { result, rerender } = renderHook(
      ({ data }) => useLabelInputs("tmpl-abort", () => data, defaultInputs, 20),
      { initialProps: { data: { tier: "standard" } } },
    );

    await waitFor(() => expect(call).toBe(1));
    rerender({ data: { tier: "pro" } });

    await waitFor(() => expect(result.current.inputs).toEqual(gatedInputs));
    expect(result.current.pending).toBe(false);
    expect(result.current.error).toBeUndefined();
    expect(call).toBe(2);
  });
});
