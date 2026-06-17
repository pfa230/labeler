import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { useState } from "react";
import { ConnectorBrowser } from "./ConnectorBrowser";
import type { ConnectorSchema, RowRef } from "../../api/connectors";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const schema: ConnectorSchema = {
  version: "homebox-1",
  resources: [
    { id: "entities", label: "Items", view: "table",
      columns: [{ key: "name", label: "Name", ty: "text", tier: "cheap" }, { key: "entityType", label: "Type", ty: "badge", tier: "cheap" }],
      filters: [{ key: "q", label: "Search", ty: "search" }] },
  ],
  relationships: [],
};

function Harness() {
  const [selected, setSelected] = useState<RowRef[]>([]);
  return (
    <div>
      <span data-testid="count">{selected.length}</span>
      <ConnectorBrowser connectionId="c1" schema={schema} selected={selected} onSelectedChange={setSelected} />
    </div>
  );
}

describe("ConnectorBrowser", () => {
  beforeEach(() => vi.unstubAllGlobals());
  afterEach(() => vi.unstubAllGlobals());

  it("loads rows and toggles selection", async () => {
    vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
      json({ rows: [
        { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", entityType: "item" } },
        { id: { resource: "entities", key: "e2" }, cells: { name: "Shelf", entityType: "location" } },
      ], next_cursor: null, has_more: false, count: 2 })));
    render(<Harness />);
    expect(await screen.findByText("Drill")).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("select entities:e1"));
    expect(screen.getByTestId("count").textContent).toBe("1");
  });

  it("sends the search filter on Apply", async () => {
    const fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
      json({ rows: [], next_cursor: null, has_more: false, count: 0 }));
    vi.stubGlobal("fetch", fetchMock);
    render(<Harness />);
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    fireEvent.change(screen.getByLabelText("Search"), { target: { value: "drill" } });
    fireEvent.click(screen.getByRole("button", { name: /apply/i }));
    await waitFor(() => {
      const last = fetchMock.mock.calls.at(-1)!;
      expect(JSON.parse((last[1]!.body) as string).filters).toEqual({ q: "drill" });
    });
  });
});
