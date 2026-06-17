import "@testing-library/jest-dom";

// Always stub object-URL handling for blob preview/download tests. This must be unconditional, not
// `if (!URL.createObjectURL)`: on Node 22 jsdom provides a real `createObjectURL` that calls
// `blob.stream()` on the Node Blob the tests pass, throwing "object.stream is not a function". Forcing
// the stub keeps the test environment deterministic across Node versions (passes on 22 and 26).
(URL as unknown as { createObjectURL: (b: Blob) => string }).createObjectURL = () => "blob:mock";
(URL as unknown as { revokeObjectURL: (u: string) => void }).revokeObjectURL = () => {};

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
