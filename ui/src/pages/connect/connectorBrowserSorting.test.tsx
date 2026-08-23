import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within, cleanup } from "@testing-library/react";
import { useState } from "react";
import { ConnectorBrowser } from "./ConnectorBrowser";
import type { ConnectorSchema, DisplayRow, SelectedRow } from "../../api/connectors";

// Tests requirement "A column header orders the loaded rows" in
// openspec/changes/issue-170-connector-grid/specs/connector-browser/spec.md against the rendered
// grid: real row order and real aria-sort values, not the sort/filter comparators in isolation
// (those are connectorSort.ts's own unit tests).

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const schema: ConnectorSchema = {
  version: "h1",
  resources: [
    {
      id: "entities",
      label: "Items",
      view: "table",
      columns: [
        { key: "name", label: "Name", ty: "text", tier: "cheap" },
        { key: "price", label: "Price", ty: "number", tier: "cheap" },
      ],
      filters: [],
    },
  ],
  relationships: [],
};

function Harness() {
  const [selected, setSelected] = useState<SelectedRow[]>([]);
  return <ConnectorBrowser connectionId="c1" schema={schema} selected={selected} onSelectedChange={setSelected} />;
}

// `price` omitted means the cell is absent from `cells` entirely, matching how a real connector
// leaves out a field rather than sending an empty string.
function row(name: string, price?: number): DisplayRow {
  return {
    id: { resource: "entities", key: name },
    cells: price === undefined ? { name } : { name, price },
  };
}

// The filter row's only content is an unlabelled <input>, which contributes no textContent, so this
// always resolves to the sortable label-row cell for the given column. Mirrors the pattern in
// connectorBrowserSortIndicator.test.tsx.
const labelHeader = (label: string) =>
  screen.getAllByRole("columnheader").find((e) => e.textContent?.includes(label))!;

const sortedHeaderCount = () =>
  screen.getAllByRole("columnheader").filter((h) => h.getAttribute("aria-sort") !== "none").length;

// Only rows with aria-rowindex are data rows (header rows carry none); sort by that index rather
// than trusting DOM order, since it's the attribute that actually encodes displayed row position.
function orderedDataRows(): HTMLElement[] {
  return screen
    .getAllByRole("row")
    .filter((r) => r.getAttribute("aria-rowindex") !== null)
    .sort((a, b) => Number(a.getAttribute("aria-rowindex")) - Number(b.getAttribute("aria-rowindex")));
}

// Column order is always [__select, name, price] for this fixture (no relationships, so no drill
// column), so the name cell is always the second gridcell in every row.
function rowNames(): string[] {
  return orderedDataRows().map((r) => within(r).getAllByRole("gridcell")[1].textContent ?? "");
}

beforeEach(() => {
  localStorage.clear();
  vi.unstubAllGlobals();
});
afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.unstubAllGlobals();
});

describe("connector browser sorting", () => {
  it("cycles a header through ascending, descending, and back to the connector's original order", async () => {
    const rows = [row("Banana", 2), row("Cherry", 3), row("Apple", 1)];
    vi.stubGlobal("fetch", vi.fn(async () => json({ rows, next_cursor: null, has_more: false, count: 3 })));
    render(<Harness />);
    await screen.findByText("Banana");
    expect(rowNames()).toEqual(["Banana", "Cherry", "Apple"]);

    fireEvent.click(labelHeader("Name"));
    await waitFor(() => expect(rowNames()).toEqual(["Apple", "Banana", "Cherry"]));
    expect(labelHeader("Name").getAttribute("aria-sort")).toBe("ascending");

    fireEvent.click(labelHeader("Name"));
    await waitFor(() => expect(rowNames()).toEqual(["Cherry", "Banana", "Apple"]));
    expect(labelHeader("Name").getAttribute("aria-sort")).toBe("descending");

    fireEvent.click(labelHeader("Name"));
    await waitFor(() => expect(rowNames()).toEqual(["Banana", "Cherry", "Apple"]));
    expect(labelHeader("Name").getAttribute("aria-sort")).toBe("none");
  });

  it("orders a number column numerically, not as text", async () => {
    // A text sort of "10", "2", "99.95" would read "10" < "2" < "99.95" lexicographically
    // (comparing the leading "1" against "2" and "9"); numeric order is 2, 10, 99.95.
    const rows = [row("A", 10), row("B", 2), row("C", 99.95)];
    vi.stubGlobal("fetch", vi.fn(async () => json({ rows, next_cursor: null, has_more: false, count: 3 })));
    render(<Harness />);
    await screen.findByText("A");

    fireEvent.click(labelHeader("Price"));
    await waitFor(() => expect(rowNames()).toEqual(["B", "A", "C"]));
  });

  it("orders rows with no value for the sorted column after every row that has one, in both directions", async () => {
    const rows = [row("A", 10), row("B"), row("C", 2)];
    vi.stubGlobal("fetch", vi.fn(async () => json({ rows, next_cursor: null, has_more: false, count: 3 })));
    render(<Harness />);
    await screen.findByText("A");

    fireEvent.click(labelHeader("Price"));
    await waitFor(() => expect(rowNames()).toEqual(["C", "A", "B"]));

    // Descending is not a plain reverse of ascending: B (blank) stays last in both directions
    // rather than jumping to the front.
    fireEvent.click(labelHeader("Price"));
    await waitFor(() => expect(rowNames()).toEqual(["A", "C", "B"]));
  });

  it("places a page appended by Load more within the current sorted order", async () => {
    const page1 = [row("A", 10), row("B", 90)];
    const page2 = [row("C", 50)];
    const fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>();
    fetchMock.mockResolvedValueOnce(json({ rows: page1, next_cursor: "cursor1", has_more: true, count: 2 }));
    fetchMock.mockResolvedValueOnce(json({ rows: page2, next_cursor: null, has_more: false, count: 3 }));
    vi.stubGlobal("fetch", fetchMock);
    render(<Harness />);
    await screen.findByText("A");
    await screen.findByText("B");

    fireEvent.click(labelHeader("Price"));
    await waitFor(() => expect(rowNames()).toEqual(["A", "B"]));

    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    // C (50) belongs between A (10) and B (90) in the still-active ascending order, not appended
    // after B.
    await waitFor(() => expect(rowNames()).toEqual(["A", "C", "B"]));
  });

  it("releases the first column's sort state when a second column is sorted", async () => {
    const rows = [row("Banana", 2), row("Apple", 1)];
    vi.stubGlobal("fetch", vi.fn(async () => json({ rows, next_cursor: null, has_more: false, count: 2 })));
    render(<Harness />);
    await screen.findByText("Banana");

    fireEvent.click(labelHeader("Name"));
    await waitFor(() => expect(labelHeader("Name").getAttribute("aria-sort")).toBe("ascending"));

    fireEvent.click(labelHeader("Price"));
    await waitFor(() => expect(labelHeader("Price").getAttribute("aria-sort")).toBe("ascending"));
    expect(labelHeader("Name").getAttribute("aria-sort")).toBe("none");
    expect(sortedHeaderCount()).toBe(1);
  });

  it("treats Ctrl- and Meta-click exactly like a plain click, never entering multi-column sorting", async () => {
    const rows = [row("Banana", 2), row("Apple", 1)];
    vi.stubGlobal("fetch", vi.fn(async () => json({ rows, next_cursor: null, has_more: false, count: 2 })));
    const { container } = render(<Harness />);
    await screen.findByText("Banana");

    fireEvent.click(labelHeader("Name"));
    await waitFor(() => expect(labelHeader("Name").getAttribute("aria-sort")).toBe("ascending"));

    fireEvent.click(labelHeader("Price"), { ctrlKey: true });
    await waitFor(() => expect(labelHeader("Price").getAttribute("aria-sort")).toBe("ascending"));
    expect(labelHeader("Name").getAttribute("aria-sort")).toBe("none");
    expect(sortedHeaderCount()).toBe(1);

    fireEvent.click(labelHeader("Name"), { metaKey: true });
    await waitFor(() => expect(labelHeader("Name").getAttribute("aria-sort")).toBe("ascending"));
    expect(labelHeader("Price").getAttribute("aria-sort")).toBe("none");
    expect(sortedHeaderCount()).toBe(1);

    // The grid's own multi-sort order badge (svar-ui/react-grid's `wx-order`, an "N" showing a
    // column's position among several active sorts) never appears: our sortMarks always carries at
    // most one entry with no `index` field, so the badge has nothing to render even if the grid's
    // own multi-sort path were reachable.
    expect(container.querySelector(".wx-order")).not.toBeInTheDocument();
  });

  it("reports aria-sort correctly, with the filter row always none and exactly one non-none header cell per sorted column", async () => {
    const rows = [row("Banana", 2), row("Apple", 1)];
    vi.stubGlobal("fetch", vi.fn(async () => json({ rows, next_cursor: null, has_more: false, count: 2 })));
    render(<Harness />);
    await screen.findByText("Banana");

    // Unsorted: every columnheader cell, label rows and filter rows alike, reports "none".
    for (const h of screen.getAllByRole("columnheader")) {
      expect(h.getAttribute("aria-sort")).toBe("none");
    }

    fireEvent.click(labelHeader("Name"));
    await waitFor(() => expect(labelHeader("Name").getAttribute("aria-sort")).toBe("ascending"));

    const nameFilterHeader = screen.getByLabelText("Filter by Name").closest('[role="columnheader"]')!;
    expect(nameFilterHeader.getAttribute("aria-sort")).toBe("none");

    const nonNone = screen.getAllByRole("columnheader").filter((h) => h.getAttribute("aria-sort") !== "none");
    expect(nonNone).toEqual([labelHeader("Name")]);

    fireEvent.click(labelHeader("Name"));
    await waitFor(() => expect(labelHeader("Name").getAttribute("aria-sort")).toBe("descending"));
    expect(screen.getAllByRole("columnheader").filter((h) => h.getAttribute("aria-sort") !== "none")).toEqual([
      labelHeader("Name"),
    ]);
  });
});
