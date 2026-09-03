import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ConnectorBrowser } from "./ConnectorBrowser";
import type { ConnectorSchema } from "../../api/connectors";

// Task 5.3: per-column filtering tests against the rendered grid. See
// openspec/changes/issue-170-connector-grid/specs/connector-browser/spec.md, requirement
// "A per-column filter narrows the loaded rows".

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
        { key: "name", label: "Name", ty: "text", tier: "cheap", multi_valued: false },
        { key: "assetId", label: "Asset ID", ty: "text", tier: "cheap", multi_valued: false },
        { key: "count", label: "Count", ty: "number", tier: "cheap", multi_valued: false },
      ],
      filters: [],
    },
  ],
  relationships: [],
};

// Five loaded rows, well under the ~15-row test viewport window (src/setupTests.ts), covering:
// - a "Saw"/"Sawzall" pair that shares a Name substring but not an Asset ID, for the AND test
// - a `count` cell holding the text "n/a" in a `number` column, for the type-independent match test
const rows = [
  { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", assetId: "A1", count: 10 } },
  { id: { resource: "entities", key: "e2" }, cells: { name: "Saw", assetId: "A2", count: 20 } },
  { id: { resource: "entities", key: "e3" }, cells: { name: "Sawzall", assetId: "B3", count: 30 } },
  { id: { resource: "entities", key: "e4" }, cells: { name: "Sander", assetId: "A4", count: 40 } },
  { id: { resource: "entities", key: "e5" }, cells: { name: "Nail Gun", assetId: "A5", count: "n/a" } },
];

function renderBrowser() {
  return render(
    <ConnectorBrowser connectionId="c1" schema={schema} selected={[]} onSelectedChange={vi.fn()} />,
  );
}

// Every column renders two `role="columnheader"` cells (a label row and a filter row); the filter
// row's <input> contributes no textContent, so matching on textContent reliably finds the label cell.
const columnHeader = (label: string) =>
  screen.getAllByRole("columnheader").find((e) => e.textContent === label)!;

// Data rows only, excluding header rows (which also carry role="row" but no aria-rowindex).
const dataRowCount = () =>
  screen.getAllByRole("row").filter((el) => el.getAttribute("aria-rowindex") !== null).length;

afterEach(() => vi.unstubAllGlobals());

describe("connector browser per-column filtering", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows, next_cursor: null, has_more: false, count: rows.length }),
      ),
    );
  });

  it("narrows to matching rows live, and clearing restores the full loaded set", async () => {
    renderBrowser();
    await screen.findByText("Drill");
    expect(dataRowCount()).toBe(5);

    const nameFilter = screen.getByLabelText("Filter by Name");
    // Uppercase needle against a cell that is not all-uppercase, and matching in the middle rather
    // than at the start: the spec requires case-insensitive, anywhere-in-the-cell matching, not a
    // case-sensitive or prefix match.
    fireEvent.change(nameFilter, { target: { value: "RILL" } });
    await waitFor(() => expect(dataRowCount()).toBe(1));
    expect(screen.getByText("Drill")).toBeInTheDocument();
    expect(screen.queryByText("Saw")).not.toBeInTheDocument();

    fireEvent.change(nameFilter, { target: { value: "" } });
    await waitFor(() => expect(dataRowCount()).toBe(5));
    expect(screen.getByText("Saw")).toBeInTheDocument();
  });

  it("combines two column filters with AND", async () => {
    renderBrowser();
    await screen.findByText("Drill");

    // "Saw" alone matches both "Saw" (e2) and "Sawzall" (e3).
    fireEvent.change(screen.getByLabelText("Filter by Name"), { target: { value: "Saw" } });
    await waitFor(() => expect(dataRowCount()).toBe(2));

    // Adding an Asset ID filter that only e2 satisfies drops e3 (satisfies Name only) but keeps
    // e2 (satisfies both).
    fireEvent.change(screen.getByLabelText("Filter by Asset ID"), { target: { value: "A2" } });
    await waitFor(() => expect(dataRowCount()).toBe(1));
    expect(screen.getByText("Saw")).toBeInTheDocument();
    expect(screen.queryByText("Sawzall")).not.toBeInTheDocument();
  });

  it("issues no browse request and leaves the cursor unchanged", async () => {
    const fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (_input, init) => {
      const body = init?.body ? JSON.parse(init.body as string) : {};
      if (body.cursor) {
        return json({
          rows: [{ id: { resource: "entities", key: "e6" }, cells: { name: "Extra", assetId: "A6", count: 60 } }],
          next_cursor: null,
          has_more: false,
          count: rows.length + 1,
        });
      }
      return json({ rows, next_cursor: "cur1", has_more: true, count: rows.length });
    });
    vi.stubGlobal("fetch", fetchMock);

    renderBrowser();
    await screen.findByText("Drill");
    expect(fetchMock).toHaveBeenCalledTimes(1);

    fireEvent.change(screen.getByLabelText("Filter by Name"), { target: { value: "Drill" } });
    await waitFor(() => expect(dataRowCount()).toBe(1));
    expect(fetchMock).toHaveBeenCalledTimes(1);

    // Clear the filter so "Load more"'s appended row is visible, then load more: the request must
    // still carry the cursor from the very first page, proving the filter never touched it.
    fireEvent.change(screen.getByLabelText("Filter by Name"), { target: { value: "" } });
    await waitFor(() => expect(dataRowCount()).toBe(5));

    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    const secondCall = fetchMock.mock.calls[1]!;
    expect(JSON.parse(secondCall[1]!.body as string).cursor).toBe("cur1");
    await waitFor(() => expect(screen.getByText("Extra")).toBeInTheDocument());
  });

  it("clears on hide and comes back empty on re-show, rather than parked", async () => {
    renderBrowser();
    await screen.findByText("Drill");

    // Narrow to e3 (Sawzall/B3) via the Asset ID filter, which hides e1 (Drill).
    fireEvent.change(screen.getByLabelText("Filter by Asset ID"), { target: { value: "B3" } });
    await waitFor(() => expect(dataRowCount()).toBe(1));
    expect(screen.queryByText("Drill")).not.toBeInTheDocument();

    // Hide the Asset ID column through the Columns picker.
    fireEvent.click(screen.getByRole("button", { name: /customize visible columns/i }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Asset ID" }));

    // The filter no longer restricts the table: the previously-hidden row is back.
    await waitFor(() => expect(dataRowCount()).toBe(5));
    expect(screen.getByText("Drill")).toBeInTheDocument();

    // Re-show the column: its filter input comes back empty, not re-armed with "B3".
    fireEvent.click(screen.getByRole("checkbox", { name: "Asset ID" }));
    await waitFor(() => expect(screen.getByLabelText("Filter by Asset ID")).toBeInTheDocument());
    expect(screen.getByLabelText("Filter by Asset ID")).toHaveValue("");
    expect(dataRowCount()).toBe(5);
  });

  it("gives each filter input an accessible name naming its column", async () => {
    renderBrowser();
    await screen.findByText("Drill");

    expect(screen.getByLabelText("Filter by Name")).toBeInTheDocument();
    expect(screen.getByLabelText("Filter by Asset ID")).toBeInTheDocument();
    expect(screen.getByLabelText("Filter by Count")).toBeInTheDocument();
  });

  it("matches the cell as displayed regardless of the column's declared type", async () => {
    renderBrowser();
    await screen.findByText("Drill");

    // `count` is a `number` column; "n/a" is unsortable there but must still be found as text.
    fireEvent.change(screen.getByLabelText("Filter by Count"), { target: { value: "n/a" } });
    await waitFor(() => expect(dataRowCount()).toBe(1));
    expect(screen.getByText("Nail Gun")).toBeInTheDocument();
  });

  it("does not disturb the active sort", async () => {
    renderBrowser();
    await screen.findByText("Drill");

    fireEvent.click(columnHeader("Name"));
    await waitFor(() => expect(columnHeader("Name").getAttribute("aria-sort")).toBe("ascending"));

    fireEvent.change(screen.getByLabelText("Filter by Asset ID"), { target: { value: "A" } });
    await waitFor(() => expect(dataRowCount()).toBeGreaterThan(0));
    expect(columnHeader("Name").getAttribute("aria-sort")).toBe("ascending");
  });
});
