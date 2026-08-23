import "@testing-library/jest-dom";

if (!URL.createObjectURL) {
  // jsdom shim for blob preview/download tests
  (URL as unknown as { createObjectURL: (b: Blob) => string }).createObjectURL = () => "blob:mock";
  (URL as unknown as { revokeObjectURL: (u: string) => void }).revokeObjectURL = () => {};
}

// jsdom lacks ResizeObserver. react-data-grid only needs it to exist, but the SVAR grid in
// ConnectorBrowser derives its rendered row window from the height this reports: it windows rows
// unconditionally (`dynamic` off does NOT disable it), so a stub that never calls back leaves
// clientHeight at 0 and renders ~3 rows whatever the data. Deliver a contentRect instead, which is
// what a real browser does.
//
// GRID_TEST_VIEWPORT_HEIGHT mirrors the height `.connector-grid-viewport` declares in theme.css, so
// tests exercise the geometry the app actually ships rather than an arbitrary large number. A test
// that needs to prove windowing engages overrides this stub locally with a small height.
// Kept in step with `.connector-grid-viewport { height: 60vh; min-height: 360px }` by construction
// rather than by a copied pixel count, so editing the stylesheet cannot silently leave tests
// exercising a geometry the app no longer ships. connectorGridViewport.test.ts pins the pairing.
export const GRID_VIEWPORT_VH = 0.6;
export const GRID_VIEWPORT_MIN_PX = 360;
export const GRID_TEST_VIEWPORT_HEIGHT = Math.max(
  GRID_VIEWPORT_MIN_PX,
  Math.round((globalThis.window?.innerHeight ?? 768) * GRID_VIEWPORT_VH),
);
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
        // react-data-grid reads `entry.contentBoxSize[0]` (lib/index.js:843), so an entry carrying
        // only contentRect throws there. Both box-size arrays are part of the real entry shape.
        const boxSize = [
          { inlineSize: GRID_TEST_VIEWPORT_WIDTH, blockSize: GRID_TEST_VIEWPORT_HEIGHT },
        ] as unknown as ReadonlyArray<ResizeObserverSize>;
        this.cb(
          [
            {
              target,
              contentRect: rect,
              contentBoxSize: boxSize,
              borderBoxSize: boxSize,
              devicePixelContentBoxSize: boxSize,
            } as unknown as ResizeObserverEntry,
          ],
          this as unknown as ResizeObserver,
        );
      });
    }
    unobserve() {}
    disconnect() {}
  };
}

// react-data-grid scrolls the selected cell into view on edit; jsdom lacks scrollIntoView.
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {};
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

