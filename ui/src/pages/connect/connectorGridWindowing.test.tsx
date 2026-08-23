import { describe, it, expect, afterEach } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/react";
import { Grid } from "@svar-ui/react-grid";
import { GRID_TEST_VIEWPORT_HEIGHT } from "../../setupTests";

// The SVAR grid windows rows off the height ResizeObserver reports, whatever `dynamic` is set to.
// setupTests reports the height `.connector-grid-viewport` actually ships, so tests see the real
// geometry. These tests pin both ends of that: enough rows render for ordinary fixtures, and
// windowing still genuinely engages rather than having been disabled outright.

const rows = (n: number) => Array.from({ length: n }, (_, i) => ({ id: i, name: `Item ${i}` }));
const columns = [{ id: "name", header: [{ text: "Name" }] }];
const dataRowCount = () =>
  screen.getAllByRole("row").filter((el) => el.getAttribute("aria-rowindex") !== null).length;

function renderGrid(n: number) {
  return render(
    <div className="connector-grid-viewport">
      <Grid data={rows(n)} columns={columns} select={false} autoRowHeight />
    </div>,
  );
}

function withObserverHeight(height: number) {
  const previous = globalThis.ResizeObserver;
  globalThis.ResizeObserver = class {
    private readonly cb: ResizeObserverCallback;
    constructor(cb: ResizeObserverCallback) {
      this.cb = cb;
    }
    observe(target: Element) {
      queueMicrotask(() =>
        this.cb(
          [{ target, contentRect: { width: 900, height } as DOMRectReadOnly } as unknown as ResizeObserverEntry],
          this as unknown as ResizeObserver,
        ),
      );
    }
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  return () => {
    globalThis.ResizeObserver = previous;
  };
}

describe("connector grid row windowing", () => {
  afterEach(cleanup);

  it("renders well beyond the 3 rows an unreporting observer would allow", async () => {
    renderGrid(12);
    await waitFor(() => expect(dataRowCount()).toBeGreaterThan(3));
    // A no-op observer leaves clientHeight at 0 and caps the window at ~3 rows regardless of data.
    // Every row of an ordinary fixture must be queryable, or row-level assertions are meaningless.
    expect(dataRowCount()).toBe(12);
    expect(screen.getByText("Item 11")).toBeInTheDocument();
  });

  it("still windows: a short viewport renders fewer rows than are loaded", async () => {
    const restore = withObserverHeight(80);
    try {
      renderGrid(60);
      await waitFor(() => expect(dataRowCount()).toBeGreaterThan(0));
      const rendered = dataRowCount();
      expect(rendered).toBeLessThan(60);
      expect(screen.queryByText("Item 59")).not.toBeInTheDocument();
    } finally {
      restore();
    }
  });

  it("reports a viewport height matching the shipped stylesheet, not an arbitrary large number", () => {
    // A shim reporting something enormous would render every row in every test and hide the fact
    // that the shipped grid windows at all.
    expect(GRID_TEST_VIEWPORT_HEIGHT).toBeLessThan(1000);
    expect(GRID_TEST_VIEWPORT_HEIGHT).toBeGreaterThanOrEqual(360);
  });
});
