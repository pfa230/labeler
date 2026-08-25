import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import { ConnectorBrowser } from "./ConnectorBrowser";
import type { ConnectorSchema, SelectedRow } from "../../api/connectors";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const schema: ConnectorSchema = {
  version: "homebox-1",
  resources: [
    {
      id: "entities",
      label: "Items",
      view: "table",
      columns: [
        { key: "name", label: "Name", ty: "text", tier: "cheap" },
        { key: "assetId", label: "Asset ID", ty: "text", tier: "cheap" },
        { key: "description", label: "Description", ty: "text", tier: "cheap" },
        { key: "manufacturer", label: "Manufacturer", ty: "text", tier: "hydrated" },
        { key: "modelNumber", label: "Model Number", ty: "text", tier: "hydrated" },
        { key: "item_url", label: "Homebox URL", ty: "text", tier: "derived" },
      ],
      filters: [{ key: "q", label: "Search", ty: "search" }],
    },
    {
      id: "locations",
      label: "Locations",
      view: "table",
      columns: [
        { key: "name", label: "Name", ty: "text", tier: "cheap" },
        { key: "itemCount", label: "Item Count", ty: "number", tier: "cheap" },
        { key: "location_url", label: "Location URL", ty: "text", tier: "derived" },
      ],
      filters: [],
    },
  ],
  relationships: [],
};

function Harness() {
  const [selected, setSelected] = useState<SelectedRow[]>([]);
  return (
    <div>
      <span data-testid="count">{selected.length}</span>
      <ConnectorBrowser connectionId="c1" schema={schema} selected={selected} onSelectedChange={setSelected} />
    </div>
  );
}

describe("ConnectorBrowser", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
  });
  afterEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
  });

  it("loads rows and toggles selection", async () => {
    vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
      json({ rows: [
        { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", assetId: "000-001" } },
        { id: { resource: "entities", key: "e2" }, cells: { name: "Shelf", assetId: "000-002" } },
      ], next_cursor: null, has_more: false, count: 2 })));
    render(<Harness />);
    expect(await screen.findByText("Drill")).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("select entities:e1"));
    expect(screen.getByTestId("count").textContent).toBe("1");
  });

  it("snapshots the row label on select and shows the summary", async () => {
    vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
      json({ rows: [
        { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", assetId: "000-001" } },
        { id: { resource: "entities", key: "e2" }, cells: { name: "Shelf", assetId: "000-002" } },
      ], next_cursor: null, has_more: false, count: 2 })));
    const onSelectedChange = vi.fn();
    render(<ConnectorBrowser connectionId="c1" schema={schema} selected={[]} onSelectedChange={onSelectedChange} />);
    await screen.findByText("Drill");
    fireEvent.click(screen.getByLabelText("select entities:e1"));
    expect(onSelectedChange).toHaveBeenCalledWith([
      expect.objectContaining({ resource: "entities", key: "e1", label: "Drill" }),
    ]);
  });

  it("renders the visible/hidden summary for a non-empty selection", async () => {
    // Asymmetric counts (2 visible, 1 hidden) so a mistaken swap of the two numbers in the
    // rendered string cannot pass unnoticed: a symmetric 1/1 fixture renders the same text either
    // way and would let that bug through.
    vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
      json({ rows: [
        { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", assetId: "000-001" } },
        { id: { resource: "entities", key: "e2" }, cells: { name: "Shelf", assetId: "000-002" } },
      ], next_cursor: null, has_more: false, count: 2 })));
    const selected: SelectedRow[] = [
      { resource: "entities", key: "e1", label: "Drill", lastSeen: 1 },
      { resource: "entities", key: "e2", label: "Shelf", lastSeen: 2 },
      { resource: "entities", key: "e9", label: "Ghost", lastSeen: 3 },
    ];
    render(<ConnectorBrowser connectionId="c1" schema={schema} selected={selected} onSelectedChange={vi.fn()} />);
    await screen.findByText("Drill");
    expect(screen.getByText("3/200 selected (2 in this view, 1 elsewhere)")).toBeInTheDocument();
  });

  it("Load more appends a second page of rows", async () => {
    const fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (_input, init) => {
      const body = init?.body ? JSON.parse(init.body as string) : {};
      if (body.cursor) {
        return json({
          rows: [{ id: { resource: "entities", key: "e2" }, cells: { name: "Shelf", assetId: "000-002" } }],
          next_cursor: null,
          has_more: false,
          count: 2,
        });
      }
      return json({
        rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill", assetId: "000-001" } }],
        next_cursor: "c2",
        has_more: true,
        count: 2,
      });
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<Harness />);
    await screen.findByText("Drill");
    expect(screen.queryByText("Shelf")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Load more" }));

    await screen.findByText("Shelf");
    // The first page's row stays present: Load more appends rather than replaces.
    expect(screen.getByText("Drill")).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    const lastCall = fetchMock.mock.calls.at(-1)!;
    expect(JSON.parse(lastCall[1]!.body as string).cursor).toBe("c2");
    // The second page reported has_more: false, so the button is gone.
    expect(screen.queryByRole("button", { name: "Load more" })).not.toBeInTheDocument();
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

  it("manages tag chips, auto-commits pending input, and sends tag array on Apply", async () => {
    const schemaWithTags: ConnectorSchema = {
      version: "homebox-1",
      resources: [
        {
          id: "entities",
          label: "Items",
          view: "table",
          columns: [{ key: "name", label: "Name", ty: "text", tier: "cheap" }],
          filters: [
            { key: "q", label: "Search", ty: "search" },
            { key: "tag", label: "Tags", ty: "label_id" },
          ],
        },
      ],
      relationships: [],
    };
    const fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
      json({ rows: [], next_cursor: null, has_more: false, count: 0 }));
    vi.stubGlobal("fetch", fetchMock);

    render(<ConnectorBrowser connectionId="c1" schema={schemaWithTags} selected={[]} onSelectedChange={vi.fn()} />);
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());

    const tagInput = screen.getByLabelText("Tags");
    // Add first tag via Add button
    fireEvent.change(tagInput, { target: { value: "tools" } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(screen.getByText("tools")).toBeInTheDocument();
    expect(tagInput).toHaveValue("");

    // Add second tag via Enter key
    fireEvent.change(tagInput, { target: { value: "cables" } });
    fireEvent.keyDown(tagInput, { key: "Enter" });
    expect(screen.getByText("cables")).toBeInTheDocument();
    expect(tagInput).toHaveValue("");

    // Add third tag by leaving it in pending input and clicking Apply (auto-commit)
    fireEvent.change(tagInput, { target: { value: "audio" } });
    fireEvent.click(screen.getByRole("button", { name: /apply/i }));

    await waitFor(() => {
      const last = fetchMock.mock.calls.at(-1)!;
      expect(JSON.parse((last[1]!.body) as string).filters).toEqual({
        tag: ["tools", "cables", "audio"],
      });
    });

    // Remove one tag
    fireEvent.click(screen.getByRole("button", { name: "Remove tag cables" }));
    expect(screen.queryByText("cables")).not.toBeInTheDocument();

    // Clear all filters
    fireEvent.click(screen.getByRole("button", { name: /clear all filters/i }));
    await waitFor(() => {
      const last = fetchMock.mock.calls.at(-1)!;
      expect(JSON.parse((last[1]!.body) as string).filters).toBeUndefined();
    });
    expect(screen.queryByText("tools")).not.toBeInTheDocument();
  });

  describe("Filter Scope Split and Clear All Filters", () => {
    it("presents source filter group with legend and scope description, and refine group caption and description", async () => {
      vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows: [
          { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", assetId: "000-001" } },
        ], next_cursor: null, has_more: false, count: 1 })));

      render(<Harness />);
      await screen.findByText("Drill");

      // Source group fieldset, legend, and description
      const fieldset = screen.getByRole("group", { name: "Source filters" });
      expect(fieldset.tagName.toLowerCase()).toBe("fieldset");
      const legend = within(fieldset).getByText("Source filters");
      expect(legend.tagName.toLowerCase()).toBe("legend");
      expect(within(fieldset).getByLabelText("Search")).toBeInTheDocument();
      expect(within(fieldset).getByText("Queries the connection and restricts the whole result. Takes effect on Apply.")).toBeInTheDocument();

      // Refine group caption and description
      expect(screen.getByText("Refine loaded rows")).toBeInTheDocument();
      expect(screen.getByText("Narrow the rows already loaded, as you type.")).toBeInTheDocument();
    });

    it("leaves Clear all filters hidden after typing and deleting in a source filter input", async () => {
      vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows: [
          { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", assetId: "000-001" } },
        ], next_cursor: null, has_more: false, count: 1 })));

      render(<Harness />);
      await screen.findByText("Drill");

      expect(screen.queryByRole("button", { name: "Clear all filters" })).not.toBeInTheDocument();

      // Type into Search draft -> Clear all filters appears
      fireEvent.change(screen.getByLabelText("Search"), { target: { value: "a" } });
      expect(screen.getByRole("button", { name: "Clear all filters" })).toBeInTheDocument();

      // Delete the character -> filterDraft becomes { q: "" }, Clear all filters hides
      fireEvent.change(screen.getByLabelText("Search"), { target: { value: "" } });
      expect(screen.queryByRole("button", { name: "Clear all filters" })).not.toBeInTheDocument();
    });

    it("associates each source filter input and column filter input with its group scope description via accessible description", async () => {
      const schemaWithTags: ConnectorSchema = {
        version: "homebox-1",
        resources: [
          {
            id: "entities",
            label: "Items",
            view: "table",
            columns: [{ key: "name", label: "Name", ty: "text", tier: "cheap" }],
            filters: [
              { key: "q", label: "Search", ty: "search" },
              { key: "tag", label: "Tags", ty: "label_id" },
            ],
          },
        ],
        relationships: [],
      };
      vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows: [
          { id: { resource: "entities", key: "e1" }, cells: { name: "Drill" } },
        ], next_cursor: null, has_more: false, count: 1 })));

      render(<ConnectorBrowser connectionId="c1" schema={schemaWithTags} selected={[]} onSelectedChange={vi.fn()} />);
      await screen.findByText("Drill");

      const searchInput = screen.getByLabelText("Search");
      const tagInput = screen.getByLabelText("Tags");
      const nameFilter = screen.getByLabelText("Filter by Name");

      expect(searchInput).toHaveAccessibleDescription("Queries the connection and restricts the whole result. Takes effect on Apply.");
      expect(tagInput).toHaveAccessibleDescription("Queries the connection and restricts the whole result. Takes effect on Apply.");
      expect(nameFilter).toHaveAccessibleDescription("Narrow the rows already loaded, as you type.");
    });

    it("renders no source filter group and no source description for filterless resources", async () => {
      vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows: [
          { id: { resource: "locations", key: "l1" }, cells: { name: "Garage", itemCount: 5 } },
        ], next_cursor: null, has_more: false, count: 1 })));

      render(<Harness />);
      await waitFor(() => expect(screen.getByRole("button", { name: "Locations" })).toBeInTheDocument());
      fireEvent.click(screen.getByRole("button", { name: "Locations" }));
      await screen.findByText("Garage");

      expect(screen.queryByText("Source filters")).not.toBeInTheDocument();
      expect(screen.queryByText("Queries the connection and restricts the whole result. Takes effect on Apply.")).not.toBeInTheDocument();
      // Refine group is still present
      expect(screen.getByText("Refine loaded rows")).toBeInTheDocument();
    });

    it("clears both source and column filters with Clear all filters", async () => {
      const fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (_input, init) => {
        const body = init?.body ? JSON.parse(init.body as string) : {};
        if (body.filters?.q) {
          return json({
            rows: [
              { id: { resource: "entities", key: "e1" }, cells: { name: "Drill 1", assetId: "000-001" } },
              { id: { resource: "entities", key: "e2" }, cells: { name: "Drill 2", assetId: "000-002" } },
            ],
            next_cursor: null,
            has_more: false,
            count: 2,
          });
        }
        return json({
          rows: [
            { id: { resource: "entities", key: "e1" }, cells: { name: "Drill 1", assetId: "000-001" } },
            { id: { resource: "entities", key: "e2" }, cells: { name: "Drill 2", assetId: "000-002" } },
            { id: { resource: "entities", key: "e3" }, cells: { name: "Saw", assetId: "000-003" } },
          ],
          next_cursor: null,
          has_more: false,
          count: 3,
        });
      });
      vi.stubGlobal("fetch", fetchMock);

      render(<Harness />);
      await screen.findByText("Drill 1");

      // Apply source filter
      fireEvent.change(screen.getByLabelText("Search"), { target: { value: "Drill" } });
      fireEvent.click(screen.getByRole("button", { name: /apply/i }));
      await screen.findByText("Drill 2");
      expect(screen.queryByText("Saw")).not.toBeInTheDocument();

      // Type column filter to narrow to Drill 1
      fireEvent.change(screen.getByLabelText("Filter by Name"), { target: { value: "1" } });
      await waitFor(() => expect(screen.queryByText("Drill 2")).not.toBeInTheDocument());
      expect(screen.getByText("Drill 1")).toBeInTheDocument();

      // Clear all filters button should be visible in utility cluster
      const clearBtn = screen.getByRole("button", { name: "Clear all filters" });
      expect(clearBtn).toBeInTheDocument();
      fireEvent.click(clearBtn);

      // Both filters cleared, full list restored
      await screen.findByText("Saw");
      expect(screen.getByText("Drill 1")).toBeInTheDocument();
      expect(screen.getByText("Drill 2")).toBeInTheDocument();
      expect(screen.getByLabelText("Search")).toHaveValue("");
      expect(screen.getByLabelText("Filter by Name")).toHaveValue("");
    });

    it("offers Clear all filters when only column filters are set", async () => {
      vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows: [
          { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", assetId: "000-001" } },
          { id: { resource: "entities", key: "e2" }, cells: { name: "Saw", assetId: "000-002" } },
        ], next_cursor: null, has_more: false, count: 2 })));

      render(<Harness />);
      await screen.findByText("Drill");

      expect(screen.queryByRole("button", { name: "Clear all filters" })).not.toBeInTheDocument();

      fireEvent.change(screen.getByLabelText("Filter by Name"), { target: { value: "Saw" } });
      await waitFor(() => expect(screen.queryByText("Drill")).not.toBeInTheDocument());

      const clearBtn = screen.getByRole("button", { name: "Clear all filters" });
      expect(clearBtn).toBeInTheDocument();
      fireEvent.click(clearBtn);

      await screen.findByText("Drill");
      expect(screen.getByLabelText("Filter by Name")).toHaveValue("");
      expect(screen.queryByRole("button", { name: "Clear all filters" })).not.toBeInTheDocument();
    });

    it("chains server-side filter and column filter", async () => {
      const fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (_input, init) => {
        const body = init?.body ? JSON.parse(init.body as string) : {};
        if (body.filters?.q === "power") {
          return json({
            rows: [
              { id: { resource: "entities", key: "e1" }, cells: { name: "Power Drill", assetId: "A1" } },
              { id: { resource: "entities", key: "e2" }, cells: { name: "Power Saw", assetId: "A2" } },
            ],
            next_cursor: null,
            has_more: false,
            count: 2,
          });
        }
        return json({
          rows: [
            { id: { resource: "entities", key: "e1" }, cells: { name: "Power Drill", assetId: "A1" } },
            { id: { resource: "entities", key: "e2" }, cells: { name: "Power Saw", assetId: "A2" } },
            { id: { resource: "entities", key: "e3" }, cells: { name: "Hand Saw", assetId: "A3" } },
          ],
          next_cursor: null,
          has_more: false,
          count: 3,
        });
      });
      vi.stubGlobal("fetch", fetchMock);

      render(<Harness />);
      await screen.findByText("Hand Saw");

      // 1. Source filter "power"
      fireEvent.change(screen.getByLabelText("Search"), { target: { value: "power" } });
      fireEvent.click(screen.getByRole("button", { name: /apply/i }));
      await screen.findByText("Power Drill");
      expect(screen.queryByText("Hand Saw")).not.toBeInTheDocument();
      expect(screen.getByText("Power Saw")).toBeInTheDocument();

      // 2. Column filter "Drill"
      fireEvent.change(screen.getByLabelText("Filter by Name"), { target: { value: "Drill" } });
      await waitFor(() => expect(screen.queryByText("Power Saw")).not.toBeInTheDocument());
      expect(screen.getByText("Power Drill")).toBeInTheDocument();
    });
  });

  describe("Column Visibility Picker", () => {
    it("renders default cheap columns initially and shows column count badge", async () => {
      vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows: [
          {
            id: { resource: "entities", key: "e1" },
            cells: { name: "Drill", assetId: "000-001", description: "Power tool", manufacturer: "DeWalt" },
          },
        ], next_cursor: null, has_more: false, count: 1 })));
      render(<Harness />);
      await screen.findByText("Drill");

      // Default cheap columns are Name, Asset ID, Description
      expect(screen.getByRole("columnheader", { name: "Name" })).toBeInTheDocument();
      expect(screen.getByRole("columnheader", { name: "Asset ID" })).toBeInTheDocument();
      expect(screen.getByRole("columnheader", { name: "Description" })).toBeInTheDocument();
      // Hydrated / derived columns are not visible by default
      expect(screen.queryByRole("columnheader", { name: "Manufacturer" })).not.toBeInTheDocument();
      expect(screen.queryByRole("columnheader", { name: "Homebox URL" })).not.toBeInTheDocument();

      // Trigger button badge shows 3/6
      expect(screen.getByRole("button", { name: /customize visible columns/i })).toHaveTextContent("Columns (3/6)");
    });

    it("toggles column visibility on click and updates table headers and data cells", async () => {
      vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows: [
          {
            id: { resource: "entities", key: "e1" },
            cells: { name: "Drill", assetId: "000-001", description: "Power tool", manufacturer: "DeWalt" },
          },
        ], next_cursor: null, has_more: false, count: 1 })));
      render(<Harness />);
      await screen.findByText("Drill");

      // Open popover
      fireEvent.click(screen.getByRole("button", { name: /customize visible columns/i }));
      expect(screen.getByText("Visible Columns")).toBeInTheDocument();

      // Check Manufacturer
      const mfgCheckbox = screen.getByRole("checkbox", { name: "Manufacturer" });
      expect(mfgCheckbox).not.toBeChecked();
      fireEvent.click(mfgCheckbox);
      expect(mfgCheckbox).toBeChecked();

      // Table now shows Manufacturer
      expect(screen.getByRole("columnheader", { name: "Manufacturer" })).toBeInTheDocument();
      expect(screen.getByText("DeWalt")).toBeInTheDocument();

      // Uncheck Description
      const descCheckbox = screen.getByRole("checkbox", { name: "Description" });
      expect(descCheckbox).toBeChecked();
      fireEvent.click(descCheckbox);
      expect(descCheckbox).not.toBeChecked();

      // Description is removed from table
      expect(screen.queryByRole("columnheader", { name: "Description" })).not.toBeInTheDocument();
      expect(screen.queryByText("Power tool")).not.toBeInTheDocument();
    });

    it("enforces minimum 1 visible column invariant by disabling the last checked checkbox", async () => {
      vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows: [], next_cursor: null, has_more: false, count: 0 })));
      render(<Harness />);
      await waitFor(() => expect(screen.getByRole("button", { name: /customize visible columns/i })).toBeInTheDocument());

      // Open popover
      fireEvent.click(screen.getByRole("button", { name: /customize visible columns/i }));

      // Uncheck Asset ID and Description so only Name remains
      fireEvent.click(screen.getByRole("checkbox", { name: "Asset ID" }));
      fireEvent.click(screen.getByRole("checkbox", { name: "Description" }));

      // Name is the sole active column: its checkbox must be disabled
      const nameCheckbox = screen.getByRole("checkbox", { name: "Name" });
      expect(nameCheckbox).toBeChecked();
      expect(nameCheckbox).toBeDisabled();

      // Attempting to click disabled checkbox should not deselect it
      fireEvent.click(nameCheckbox);
      expect(nameCheckbox).toBeChecked();
      expect(screen.getByRole("columnheader", { name: "Name" })).toBeInTheDocument();
    });

    it("handles All and Reset quick action buttons and persists changes", async () => {
      vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows: [], next_cursor: null, has_more: false, count: 0 })));
      render(<Harness />);
      await waitFor(() => expect(screen.getByRole("button", { name: /customize visible columns/i })).toBeInTheDocument());

      fireEvent.click(screen.getByRole("button", { name: /customize visible columns/i }));

      // Click "All"
      fireEvent.click(screen.getByRole("button", { name: "All" }));
      expect(screen.getByRole("button", { name: /customize visible columns/i })).toHaveTextContent("Columns (6/6)");
      expect(screen.getByRole("columnheader", { name: "Manufacturer" })).toBeInTheDocument();
      expect(screen.getByRole("columnheader", { name: "Homebox URL" })).toBeInTheDocument();

      // Verify localStorage was updated
      const rawStored = localStorage.getItem("labeler:connector-columns:c1:entities");
      expect(rawStored).toBeTruthy();
      const parsed = JSON.parse(rawStored!);
      expect(parsed).toHaveLength(6);

      // Click "Reset" to revert to cheap defaults
      fireEvent.click(screen.getByRole("button", { name: "Reset" }));
      expect(screen.getByRole("button", { name: /customize visible columns/i })).toHaveTextContent("Columns (3/6)");
      expect(screen.queryByRole("columnheader", { name: "Manufacturer" })).not.toBeInTheDocument();
    });

    it("restores persisted column preferences across component remounts", async () => {
      localStorage.setItem("labeler:connector-columns:c1:entities", JSON.stringify(["name", "manufacturer"]));

      vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows: [
          {
            id: { resource: "entities", key: "e1" },
            cells: { name: "Drill", assetId: "000-001", description: "Power tool", manufacturer: "DeWalt" },
          },
        ], next_cursor: null, has_more: false, count: 1 })));

      const { unmount } = render(<Harness />);
      await screen.findByText("Drill");

      // Stored columns are Name and Manufacturer
      expect(screen.getByRole("columnheader", { name: "Name" })).toBeInTheDocument();
      expect(screen.getByRole("columnheader", { name: "Manufacturer" })).toBeInTheDocument();
      expect(screen.queryByRole("columnheader", { name: "Asset ID" })).not.toBeInTheDocument();
      expect(screen.queryByRole("columnheader", { name: "Description" })).not.toBeInTheDocument();

      unmount();
    });

    it("works seamlessly on filterless resources like locations", async () => {
      vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows: [
          { id: { resource: "locations", key: "l1" }, cells: { name: "Garage", itemCount: 5 } },
        ], next_cursor: null, has_more: false, count: 1 })));

      render(<Harness />);
      await waitFor(() => expect(screen.getByRole("button", { name: "Locations" })).toBeInTheDocument());

      // Switch to Locations
      fireEvent.click(screen.getByRole("button", { name: "Locations" }));
      await screen.findByText("Garage");

      // Column picker is available on filterless resource
      expect(screen.getByRole("button", { name: /customize visible columns/i })).toHaveTextContent("Columns (2/3)");
      expect(screen.getByRole("columnheader", { name: "Name" })).toBeInTheDocument();
      expect(screen.getByRole("columnheader", { name: "Item Count" })).toBeInTheDocument();
      expect(screen.queryByRole("columnheader", { name: "Location URL" })).not.toBeInTheDocument();
    });

    it("dismisses the column picker popover on Escape key and outside click", async () => {
      vi.stubGlobal("fetch", vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () =>
        json({ rows: [], next_cursor: null, has_more: false, count: 0 })));
      render(<Harness />);
      await waitFor(() => expect(screen.getByRole("button", { name: /customize visible columns/i })).toBeInTheDocument());

      // Open popover
      fireEvent.click(screen.getByRole("button", { name: /customize visible columns/i }));
      expect(screen.getByText("Visible Columns")).toBeInTheDocument();

      // Press Escape
      fireEvent.keyDown(document, { key: "Escape" });
      expect(screen.queryByText("Visible Columns")).not.toBeInTheDocument();

      // Open popover again
      fireEvent.click(screen.getByRole("button", { name: /customize visible columns/i }));
      expect(screen.getByText("Visible Columns")).toBeInTheDocument();

      // Click outside
      fireEvent.pointerDown(document.body);
      expect(screen.queryByText("Visible Columns")).not.toBeInTheDocument();
    });
  });
});
