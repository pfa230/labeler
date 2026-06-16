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
