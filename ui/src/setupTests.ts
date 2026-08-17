import "@testing-library/jest-dom";

if (!URL.createObjectURL) {
  // jsdom shim for blob preview/download tests
  (URL as unknown as { createObjectURL: (b: Blob) => string }).createObjectURL = () => "blob:mock";
  (URL as unknown as { revokeObjectURL: (u: string) => void }).revokeObjectURL = () => {};
}

// react-data-grid uses ResizeObserver for column sizing; jsdom lacks it.
if (!("ResizeObserver" in globalThis)) {
  (globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = class {
    observe() {}
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

