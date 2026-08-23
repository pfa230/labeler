import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import { ConnectorBrowser } from "./ConnectorBrowser";
import type { ConnectorSchema, DisplayRow, SelectedRow } from "../../api/connectors";

// Covers OpenSpec issue-170-connector-grid, requirement "The browse table's existing behavior is
// preserved under the view controls" (openspec/changes/issue-170-connector-grid/specs/
// connector-browser/spec.md). Sorting/filtering are new; everything in this file asserts that the
// pre-existing contract around them (selection identity, the summary split, the materialize cap,
// row links, and drill-down) still holds once they're active.

const MATERIALIZE_CAP = 200;

const schema: ConnectorSchema = {
  version: "h1",
  resources: [
    {
      id: "entities",
      label: "Items",
      view: "table",
      columns: [
        { key: "name", label: "Name", ty: "text", tier: "cheap" },
        { key: "assetId", label: "Asset ID", ty: "text", tier: "cheap" },
      ],
      filters: [],
    },
    {
      id: "parts",
      label: "Parts",
      view: "table",
      columns: [{ key: "name", label: "Name", ty: "text", tier: "cheap" }],
      filters: [],
    },
  ],
  relationships: [{ id: "children", label: "Children", from: "entities", to: "parts" }],
};

// Order deliberately does not match alphabetical (name) order, so an ascending sort provably moves
// rows rather than happening to leave them in place. Saw (index 1) ends up at index 3 post-sort.
const entityRows: DisplayRow[] = [
  { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", assetId: "A1" }, url: "https://src.example/e1" },
  { id: { resource: "entities", key: "e2" }, cells: { name: "Saw", assetId: "B2" }, url: "https://src.example/e2" },
  { id: { resource: "entities", key: "e3" }, cells: { name: "Nail", assetId: "C3" }, url: "https://src.example/e3" },
  { id: { resource: "entities", key: "e4" }, cells: { name: "Screw", assetId: "D4" }, url: "https://src.example/e4" },
  { id: { resource: "entities", key: "e5" }, cells: { name: "Hammer", assetId: "E5" }, url: "https://src.example/e5" },
];

const partRows: DisplayRow[] = [{ id: { resource: "parts", key: "p1" }, cells: { name: "Bit" } }];

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

function makeFetchMock() {
  return vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (_input, init) => {
    const body = init?.body ? JSON.parse(init.body as string) : {};
    const rows = body.resource === "parts" ? partRows : entityRows;
    return json({ rows, next_cursor: null, has_more: false, count: rows.length });
  });
}

// Mirrors connectorBrowserSortIndicator.test.tsx: the label header cell is the only
// `role="columnheader"` whose textContent carries the label (the filter row's header cell holds an
// <input>, which contributes no text content).
const columnHeader = (label: string) => screen.getAllByRole("columnheader").find((e) => e.textContent?.includes(label))!;

const sortByName = () => fireEvent.click(columnHeader("Name"));

// Data rows carry `aria-rowindex`; header rows (role="row" too) do not. Restricting to data rows
// keeps this from ever matching a header cell's incidental class list.
function dataRows(): HTMLElement[] {
  return screen.getAllByRole("row").filter((el) => el.getAttribute("aria-rowindex") !== null);
}

// The one check this whole file exists to make trustworthy: whether SVAR's own selection model
// (`wx-selected`, exec("select-row", ...)) ever activated, independent of the app's own `selected`
// state. See ConnectorBrowser.tsx's `select={false}` on <Grid>.
function anySvarRowSelected(): boolean {
  return dataRows().some((el) => el.className.split(/\s+/).includes("wx-selected"));
}

function Harness() {
  const [selected, setSelected] = useState<SelectedRow[]>([]);
  return (
    <div>
      <span data-testid="count">{selected.length}</span>
      <ConnectorBrowser connectionId="c1" schema={schema} selected={selected} onSelectedChange={setSelected} />
    </div>
  );
}

describe("ConnectorBrowser: existing contract preserved under view controls", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
  });
  afterEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
  });

  it("keeps a filtered-out row selected, in the summary, and removable from it", async () => {
    vi.stubGlobal("fetch", makeFetchMock());
    render(<Harness />);
    await screen.findByText("Drill");

    fireEvent.click(screen.getByLabelText("select entities:e2"));
    expect(screen.getByTestId("count").textContent).toBe("1");

    fireEvent.change(screen.getByLabelText("Filter by Name"), { target: { value: "Drill" } });
    await waitFor(() => expect(screen.queryByLabelText("select entities:e2")).not.toBeInTheDocument());

    // Still selected: loaded-but-hidden counts as "in this view", not "elsewhere".
    expect(screen.getByText("1/200 selected (1 in this view, 0 elsewhere)")).toBeInTheDocument();
    expect(screen.getByText("Saw")).toBeInTheDocument(); // the summary chip

    fireEvent.click(screen.getByRole("button", { name: "remove Saw" }));
    expect(screen.getByTestId("count").textContent).toBe("0");
    expect(screen.queryByText("Saw")).not.toBeInTheDocument();
  });

  it("does not move rows between the summary's in-view/elsewhere halves when filtering", async () => {
    vi.stubGlobal("fetch", makeFetchMock());
    const selected: SelectedRow[] = [
      { resource: "entities", key: "e2", label: "Saw", lastSeen: 1 }, // loaded
      { resource: "entities", key: "e9", label: "Ghost", lastSeen: 2 }, // never loaded
    ];
    render(<ConnectorBrowser connectionId="c1" schema={schema} selected={selected} onSelectedChange={vi.fn()} />);
    await screen.findByText("Drill");

    const before = screen.getByText(/\/200 selected \(/).textContent;
    expect(before).toBe("2/200 selected (1 in this view, 1 elsewhere)");

    fireEvent.change(screen.getByLabelText("Filter by Name"), { target: { value: "Nail" } });
    await waitFor(() => expect(screen.queryByLabelText("select entities:e1")).not.toBeInTheDocument());

    const after = screen.getByText(/\/200 selected \(/).textContent;
    expect(after).toBe(before);
  });

  it("keeps the materialize cap binding, and a selected row's control enabled, while sorted", async () => {
    vi.stubGlobal("fetch", makeFetchMock());
    // 200 is impractical to reach by clicking checkboxes in a unit test, per the task brief: assert
    // the `disabled` predicate directly against a large `selected` prop instead.
    const cappedSelected: SelectedRow[] = [
      { resource: "entities", key: "e2", label: "Saw", lastSeen: 1 },
      ...Array.from({ length: MATERIALIZE_CAP - 1 }, (_, i) => ({
        resource: "filler",
        key: `f${i}`,
        label: `Filler ${i}`,
        lastSeen: i + 2,
      })),
    ];
    expect(cappedSelected).toHaveLength(MATERIALIZE_CAP);
    render(<ConnectorBrowser connectionId="c1" schema={schema} selected={cappedSelected} onSelectedChange={vi.fn()} />);
    await screen.findByText("Drill");

    sortByName();
    await waitFor(() => expect(columnHeader("Name").getAttribute("aria-sort")).toBe("ascending"));

    const selectedCheckbox = screen.getByLabelText("select entities:e2");
    expect(selectedCheckbox).toBeChecked();
    expect(selectedCheckbox).not.toBeDisabled();

    for (const key of ["e1", "e3", "e4", "e5"]) {
      const checkbox = screen.getByLabelText(`select entities:${key}`);
      expect(checkbox).not.toBeChecked();
      expect(checkbox).toBeDisabled();
    }
  });

  it("keeps a row selected by identity when a sort moves it", async () => {
    vi.stubGlobal("fetch", makeFetchMock());
    render(<Harness />);
    await screen.findByText("Drill");

    fireEvent.click(screen.getByLabelText("select entities:e2")); // Saw, at connector-order index 1
    expect(screen.getByTestId("count").textContent).toBe("1");

    sortByName();
    await waitFor(() => expect(columnHeader("Name").getAttribute("aria-sort")).toBe("ascending"));

    // Ascending order is Drill, Hammer, Nail, Saw, Screw: Saw moved from index 1 to index 3.
    // Read the name cell's own link text rather than the row's whole textContent: every row also
    // carries a "Drill in" button, whose label is a substring of the row named "Drill".
    const names = dataRows().map((row) => within(row).getByRole("link").textContent);
    expect(names).toEqual(["Drill", "Hammer", "Nail", "Saw", "Screw"]);

    const sawCheckbox = screen.getByLabelText("select entities:e2");
    expect(sawCheckbox).toBeChecked();
    expect(screen.getByTestId("count").textContent).toBe("1");
  });

  it("keeps each row's source link and drill-in action while a sort is active", async () => {
    vi.stubGlobal("fetch", makeFetchMock());
    render(<ConnectorBrowser connectionId="c1" schema={schema} selected={[]} onSelectedChange={vi.fn()} />);
    await screen.findByText("Drill");

    sortByName();
    await waitFor(() => expect(columnHeader("Name").getAttribute("aria-sort")).toBe("ascending"));

    for (const row of entityRows) {
      const link = screen.getByRole("link", { name: row.cells.name as string });
      expect(link).toHaveAttribute("href", row.url);
      expect(link).toHaveAttribute("target", "_blank");
      expect(link).toHaveAttribute("rel", "noopener");
    }
    expect(screen.getAllByRole("button", { name: "Drill in" })).toHaveLength(entityRows.length);
  });

  it("still drills in and lands on the related resource while a sort and a filter are active", async () => {
    vi.stubGlobal("fetch", makeFetchMock());
    render(<ConnectorBrowser connectionId="c1" schema={schema} selected={[]} onSelectedChange={vi.fn()} />);
    await screen.findByText("Drill");

    sortByName();
    await waitFor(() => expect(columnHeader("Name").getAttribute("aria-sort")).toBe("ascending"));

    fireEvent.change(screen.getByLabelText("Filter by Name"), { target: { value: "Drill" } });
    await waitFor(() => expect(screen.queryByText("Saw")).not.toBeInTheDocument());
    expect(screen.getByText("Drill")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Drill in" }));

    await screen.findByText("Bit");
    expect(screen.getByText(/in Drill/)).toBeInTheDocument();
  });

  // THE MOST IMPORTANT TEST IN THIS FILE. SVAR's own click handler ignores exactly one thing: a
  // click landing inside an <input> (`g.target.closest("input")`), which is what a checkbox click
  // is. It does NOT ignore a click on an ordinary cell, the name link, or a button in the row - the
  // component's `select={false}` prop is what stops those from reaching SVAR's `select-row` and
  // adding `wx-selected`. A test that only ever clicks checkboxes can never observe this: it clicks
  // the one element SVAR's own handler already ignores, and would pass even with `select={false}`
  // removed. See @svar-ui/react-grid dist/index.es.js: `click: (o, g) => { if
  // (g.target.closest("input") || ...) return; ...; if (C === !1) return; ...exec("select-row",...) }`
  // and @svar-ui/grid-store dist/index.js's keydown handler, which gates arrow-key select-row calls
  // on that same `select` flag.
  it("never activates SVAR's own row selection: ordinary cells, the name link, drill-in, and keyboard", async () => {
    vi.stubGlobal("fetch", makeFetchMock());
    render(<Harness />);
    await screen.findByText("Drill");
    expect(anySvarRowSelected()).toBe(false);

    // 1. An ordinary data cell (not the name link, not a checkbox).
    const ordinaryCell = screen.getByText("C3"); // Nail's Asset ID
    fireEvent.click(ordinaryCell);
    expect(anySvarRowSelected()).toBe(false);
    expect(screen.getByTestId("count").textContent).toBe("0");

    // 2. The name cell's link.
    const nameLink = screen.getByRole("link", { name: "Drill" });
    fireEvent.click(nameLink);
    expect(anySvarRowSelected()).toBe(false);
    expect(screen.getByTestId("count").textContent).toBe("0");

    // 3. Keyboard activation: arrow-key navigation is the one keyboard path SVAR routes through
    // select-row (see grid-store's keydown handler, gated on the same `select` flag as the click
    // handler). Fire it from a cell already inside the grid so it reaches the grid's key scope.
    fireEvent.keyDown(ordinaryCell, { key: "ArrowDown", code: "ArrowDown" });
    fireEvent.keyDown(ordinaryCell, { key: "ArrowUp", code: "ArrowUp" });
    expect(anySvarRowSelected()).toBe(false);
    expect(screen.getByTestId("count").textContent).toBe("0");

    // 4. Drill in: legitimately navigates (the app's own affordance), but must not select in SVAR's
    // model, and must not touch the app's own `selected` array either.
    const hammerRow = dataRows().find((row) => row.textContent?.includes("Hammer"))!;
    fireEvent.click(within(hammerRow).getByRole("button", { name: "Drill in" }));
    await screen.findByText("Bit");
    expect(anySvarRowSelected()).toBe(false);
    expect(screen.getByTestId("count").textContent).toBe("0");
  });
});
