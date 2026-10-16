import "@testing-library/jest-dom";

if (!URL.createObjectURL) {
  // jsdom shim for blob preview/download tests
  (URL as unknown as { createObjectURL: (b: Blob) => string }).createObjectURL = () => "blob:mock";
  (URL as unknown as { revokeObjectURL: (u: string) => void }).revokeObjectURL = () => {};
}

// jsdom lacks ResizeObserver, and the SVAR grid derives its rendered row window from the height it
// reports: it windows rows unconditionally (`dynamic` off does NOT disable it), so a stub that never
// calls back leaves clientHeight at 0 and renders ~3 rows whatever the data. Deliver a contentRect
// instead, which is what a real browser does.
//
// Every observed element gets one fixed viewport, tall enough for a realistic row window. It is not
// the geometry the app ships — `.label-grid-viewport` declares 350px in theme.css, which
// gridViewport.test.ts pins — so no test should read this stub as that geometry, and none asserts
// on the row window it produces. A test that needs to prove windowing engages overrides this stub
// locally with a small height.
export const GRID_TEST_VIEWPORT_HEIGHT = 461;
export const GRID_TEST_VIEWPORT_WIDTH = 900;

if (!("ResizeObserver" in globalThis)) {
  (globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = class {
    private readonly cb: ResizeObserverCallback;
    constructor(cb: ResizeObserverCallback) {
      this.cb = cb;
    }
    observe(target: Element) {
      // Asynchronous, like the real thing: the grid sets state from this callback, and firing it
      // synchronously inside observe() would run that during render.
      queueMicrotask(() => {
        const rect = {
          width: GRID_TEST_VIEWPORT_WIDTH,
          height: GRID_TEST_VIEWPORT_HEIGHT,
          top: 0,
          left: 0,
          right: GRID_TEST_VIEWPORT_WIDTH,
          bottom: GRID_TEST_VIEWPORT_HEIGHT,
          x: 0,
          y: 0,
        } as DOMRectReadOnly;
        this.cb(
          [
            { target, contentRect: rect } as unknown as ResizeObserverEntry,
          ],
          this as unknown as ResizeObserver,
        );
      });
    }
    unobserve() {}
    disconnect() {}
  };
}

// jsdom lacks matchMedia. Default to DESKTOP (matches: true) so existing tests keep the
// always-visible preview behavior; mobile-specific tests override this stub locally.
if (typeof window !== "undefined" && !window.matchMedia) {
  window.matchMedia = (query: string): MediaQueryList =>
    ({
      matches: true,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }) as MediaQueryList;
}

// Node 22+ defines an experimental global localStorage that evaluates to undefined without flags,
// breaking jsdom's window.localStorage.
if (typeof window !== "undefined" && (!window.localStorage || typeof window.localStorage.clear !== "function")) {
  let store: Record<string, string> = {};
  const mockStorage: Storage = {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, value: string) => {
      store[key] = String(value);
    },
    removeItem: (key: string) => {
      delete store[key];
    },
    clear: () => {
      store = {};
    },
    get length() {
      return Object.keys(store).length;
    },
    key: (index: number) => Object.keys(store)[index] ?? null,
  };
  Object.defineProperty(window, "localStorage", {
    value: mockStorage,
    configurable: true,
    writable: true,
  });
  Object.defineProperty(globalThis, "localStorage", {
    value: mockStorage,
    configurable: true,
    writable: true,
  });
}

// The sheet badge's icon cells as x/y/width/height (#201). Templates.test.tsx and
// TemplateDetail.test.tsx both assert against this: the requirement is that the two pages render one
// badge, so comparing each against the same expectation is what proves they have not diverged.
export const SHEET_ICON = ["2,0,3,3", "7,0,3,3", "2,4.5,3,3", "7,4.5,3,3", "2,9,3,3", "7,9,3,3"];

// The single icon's one cell, landscape against the sheet's portrait. Compared at both call sites for
// the same reason: a rect count of 1 says nothing about whether the two pages drew the same shape.
export const SINGLE_ICON = ["0,3,12,6"];

export function iconGeometry(badge: HTMLElement): string[] {
  return Array.from(badge.querySelectorAll("rect")).map((r) =>
    ["x", "y", "width", "height"].map((a) => r.getAttribute(a)).join(","),
  );
}

// A prose mention of a format must not acquire the badge's colour treatment (#201). Rejecting only
// the foreground token was not enough: an --info-soft fill with ink text and no marker would have
// passed while looking exactly like a badge. All four of the badge's tokens are rejected, foreground
// and fill alike. This reads the element's own inline style, which is how every badge colour in this
// app is applied; it does not chase an inherited or class-applied colour.
export function noBadgeStyling(el: HTMLElement): boolean {
  const style = el.getAttribute("style") ?? "";
  return !/--info\b|--info-soft\b|--accent\b|--accent-soft\b/.test(style);
}
