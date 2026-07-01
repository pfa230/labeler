import { describe, it, expect, vi, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useMediaQuery } from "./useMediaQuery";

function stubMatchMedia(initial: boolean) {
  let listener: ((e: { matches: boolean }) => void) | null = null;
  const mql = {
    matches: initial,
    media: "(min-width: 1024px)",
    addEventListener: (_: string, fn: (e: { matches: boolean }) => void) => {
      listener = fn;
    },
    removeEventListener: () => {
      listener = null;
    },
  };
  vi.stubGlobal("matchMedia", () => mql as unknown as MediaQueryList);
  return { fire: (matches: boolean) => act(() => listener?.({ matches })) };
}

afterEach(() => vi.unstubAllGlobals());

describe("useMediaQuery", () => {
  it("returns the current match and tracks changes", () => {
    const ctl = stubMatchMedia(false);
    const { result } = renderHook(() => useMediaQuery("(min-width: 1024px)"));
    expect(result.current).toBe(false);
    ctl.fire(true);
    expect(result.current).toBe(true);
  });
});
