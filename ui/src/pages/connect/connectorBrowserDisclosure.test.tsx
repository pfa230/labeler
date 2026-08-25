import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { useState } from "react";
import { ConnectorBrowser } from "./ConnectorBrowser";
import type { ConnectorSchema, SelectedRow } from "../../api/connectors";

// The three disclosure strings are fixed by design.md "Disclosure copy is specified, not left to
// taste" and by the spec requirement "Sorting and filtering act on loaded rows, and the table says
// so". They are asserted verbatim throughout this file rather than via substring/regex, so a
// wording drift in either the copy or the numbers fails loudly.
const SHOWING = (shown: number, loaded: number) => `Showing ${shown} of ${loaded} loaded rows`;
const LOADED_SO_FAR = (loaded: number) => `Sorting and refining cover only the ${loaded} rows loaded so far`;
const NO_MATCH_WITH_MORE = "No loaded row matches. More rows can be loaded.";

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
  ],
  relationships: [],
};

// Six rows, two of which ("Saw", "Sawzall") share a "Saw" prefix so a filter can narrow from 6 -> 2
// -> 1 without needing a larger fixture. Well under the ~15-row window the grid renders at the
// setupTests.ts stub height, so every row here is actually in the DOM.
const rows = [
  { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", assetId: "A1" } },
  { id: { resource: "entities", key: "e2" }, cells: { name: "Saw", assetId: "A2" } },
  { id: { resource: "entities", key: "e3" }, cells: { name: "Sawzall", assetId: "A3" } },
  { id: { resource: "entities", key: "e4" }, cells: { name: "Sander", assetId: "A4" } },
  { id: { resource: "entities", key: "e5" }, cells: { name: "Wrench", assetId: "W1" } },
  { id: { resource: "entities", key: "e6" }, cells: { name: "Hammer", assetId: "H1" } },
];

function stubBrowse(hasMore: boolean) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () =>
      new Response(JSON.stringify({ rows, next_cursor: hasMore ? "cursor-1" : null, has_more: hasMore, count: rows.length }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    ),
  );
}

function H() {
  const [s, set] = useState<SelectedRow[]>([]);
  return <ConnectorBrowser connectionId="c1" schema={schema} selected={s} onSelectedChange={set} />;
}

const nameFilter = () => screen.getByLabelText("Filter by Name") as HTMLInputElement;

afterEach(() => vi.unstubAllGlobals());

describe("scope disclosure", () => {
  it("states the filtered subset when a filter is active and no more rows remain", async () => {
    stubBrowse(false);
    render(<H />);
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());

    fireEvent.change(nameFilter(), { target: { value: "Saw" } });

    await waitFor(() => expect(screen.getByText(SHOWING(2, 6))).toBeInTheDocument());
    expect(screen.queryByText(LOADED_SO_FAR(6))).not.toBeInTheDocument();
    expect(screen.queryByText(NO_MATCH_WITH_MORE)).not.toBeInTheDocument();
  });

  it("states more rows are available when has_more is true and no filter is active", async () => {
    stubBrowse(true);
    render(<H />);
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());

    await waitFor(() => expect(screen.getByText(LOADED_SO_FAR(6))).toBeInTheDocument());
    expect(screen.queryByText(/^Showing \d+ of \d+ loaded rows$/)).not.toBeInTheDocument();
    expect(screen.queryByText(NO_MATCH_WITH_MORE)).not.toBeInTheDocument();
  });

  it("shows both lines together, in one container, when both conditions hold", async () => {
    stubBrowse(true);
    render(<H />);
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());

    fireEvent.change(nameFilter(), { target: { value: "Saw" } });

    await waitFor(() => expect(screen.getByText(SHOWING(2, 6))).toBeInTheDocument());
    const showing = screen.getByText(SHOWING(2, 6));
    const loadedSoFar = screen.getByText(LOADED_SO_FAR(6));
    const status = screen.getByRole("status");
    expect(status).toContainElement(showing);
    expect(status).toContainElement(loadedSoFar);
    expect(screen.queryByText(NO_MATCH_WITH_MORE)).not.toBeInTheDocument();
  });

  it("replaces the other two lines with the no-match message when a filter matches nothing and more rows are available", async () => {
    stubBrowse(true);
    render(<H />);
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());

    fireEvent.change(nameFilter(), { target: { value: "zzz-no-match" } });

    await waitFor(() => expect(screen.getByText(NO_MATCH_WITH_MORE)).toBeInTheDocument());
    expect(screen.queryByText(/^Showing /)).not.toBeInTheDocument();
    expect(screen.queryByText(/loaded so far$/)).not.toBeInTheDocument();
    // The user is not left looking at a bare empty grid: the status line is the only thing telling
    // them why nothing is shown.
    expect(screen.getByRole("status")).toHaveTextContent(NO_MATCH_WITH_MORE);
  });

  it("renders no status container at all when neither condition holds", async () => {
    stubBrowse(false);
    render(<H />);
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());

    // Not merely empty text: the container itself must be absent.
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("marks the disclosure container role=status so changes are announced politely", async () => {
    stubBrowse(false);
    render(<H />);
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());

    fireEvent.change(nameFilter(), { target: { value: "Saw" } });

    await waitFor(() => expect(screen.getByRole("status")).toBeInTheDocument());
  });

  it("updates the shown count live as the filter narrows", async () => {
    stubBrowse(false);
    render(<H />);
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());

    fireEvent.change(nameFilter(), { target: { value: "Saw" } });
    await waitFor(() => expect(screen.getByText(SHOWING(2, 6))).toBeInTheDocument());

    fireEvent.change(nameFilter(), { target: { value: "Sawz" } });
    await waitFor(() => expect(screen.getByText(SHOWING(1, 6))).toBeInTheDocument());
    expect(screen.queryByText(SHOWING(2, 6))).not.toBeInTheDocument();
  });

  it("treats only a non-empty filter as active: clearing, and a whitespace-only needle, both return to no disclosure", async () => {
    stubBrowse(false);
    render(<H />);
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());

    fireEvent.change(nameFilter(), { target: { value: "Saw" } });
    await waitFor(() => expect(screen.getByText(SHOWING(2, 6))).toBeInTheDocument());

    fireEvent.change(nameFilter(), { target: { value: "" } });
    await waitFor(() => expect(screen.queryByRole("status")).not.toBeInTheDocument());

    fireEvent.change(nameFilter(), { target: { value: "   " } });
    // A whitespace-only needle must not count as an active filter either: the container stays
    // absent rather than reappearing with a "Showing 0 of 6" (or similar) line.
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("renders the disclosure container after the grid in DOM order", async () => {
    stubBrowse(true);
    const { container } = render(<H />);
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());

    const grid = container.querySelector(".connector-grid-viewport")!;
    const status = screen.getByRole("status");
    expect(grid).toBeInTheDocument();
    expect(status).toBeInTheDocument();
    // Grid must come before the status/disclosure element in document order
    expect(grid.compareDocumentPosition(status) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("still shows the partial-load statement when a source filter is applied and has_more is true", async () => {
    const schemaWithFilter: ConnectorSchema = {
      version: "h1",
      resources: [
        {
          id: "entities",
          label: "Items",
          view: "table",
          columns: [{ key: "name", label: "Name", ty: "text", tier: "cheap" }],
          filters: [{ key: "q", label: "Search", ty: "search" }],
        },
      ],
      relationships: [],
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(
          JSON.stringify({ rows, next_cursor: "cur-1", has_more: true, count: rows.length }),
          { status: 200, headers: { "content-type": "application/json" } },
        ),
      ),
    );
    render(<ConnectorBrowser connectionId="c1" schema={schemaWithFilter} selected={[]} onSelectedChange={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("Drill")).toBeInTheDocument());

    // Apply a source filter
    fireEvent.change(screen.getByLabelText("Search"), { target: { value: "drill" } });
    fireEvent.click(screen.getByRole("button", { name: /apply/i }));

    await waitFor(() => expect(screen.getByText(LOADED_SO_FAR(6))).toBeInTheDocument());
  });
});
