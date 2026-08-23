import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { useState } from "react";
import { ConnectorBrowser } from "./ConnectorBrowser";
import type { ConnectorSchema, SelectedRow } from "../../api/connectors";
const schema: ConnectorSchema = { version: "h1", resources: [{ id: "entities", label: "Items", view: "table",
  columns: [ { key: "name", label: "Name", ty: "text", tier: "cheap" }, { key: "assetId", label: "Asset ID", ty: "text", tier: "cheap" } ], filters: [] }], relationships: [] };
const rows = [ { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", assetId: "A1" } },
               { id: { resource: "entities", key: "e2" }, cells: { name: "Saw", assetId: "B2" } } ];
function H() { const [s, set] = useState<SelectedRow[]>([]); return <ConnectorBrowser connectionId="c1" schema={schema} selected={s} onSelectedChange={set} />; }
const nameHeader = () => screen.getAllByRole("columnheader").find((e) => e.textContent?.includes("Name"))!;
afterEach(() => vi.unstubAllGlobals());
// Regression guard. `DataStore.init` clears `sortMarks` whenever the data identity changes and will
// not re-apply a `sortMarks` prop it considers unchanged by reference, so deriving the marks
// separately from the rows leaves the rows sorted while the header reports aria-sort="none". The
// spec requires the sorted column be conveyed to assistive technology, and a header that claims
// "none" over sorted rows is worse than one that says nothing.
describe("sort indicator", () => {
  it("survives a filter edit on another column", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ rows, next_cursor: null, has_more: false, count: 2 }), { status: 200, headers: { "content-type": "application/json" } })));
    render(<H />);
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());
    fireEvent.click(nameHeader());
    await waitFor(() => expect(nameHeader().getAttribute("aria-sort")).toBe("ascending"));
    console.log("ARIA after sort:", nameHeader().getAttribute("aria-sort"));
    fireEvent.change(screen.getByLabelText("Filter by Asset ID"), { target: { value: "A" } });
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());
    console.log("ARIA after filtering another column:", nameHeader().getAttribute("aria-sort"));
    expect(nameHeader().getAttribute("aria-sort")).toBe("ascending");
  });
});
