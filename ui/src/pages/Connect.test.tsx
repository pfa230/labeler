import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { ToastProvider } from "../app/toast";
import { Connect } from "./Connect";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const schema = {
  version: "homebox-1",
  resources: [{ id: "entities", label: "Items", view: "table",
    columns: [{ key: "name", label: "Name", ty: "text", tier: "cheap" }], filters: [] }],
  relationships: [],
};
const templateDetail = {
  id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300,
  format: { type: "single" }, options: {},
  layout: [{ type: "text", value: "{name}" }],
};

type StubOptions = {
  renderLabel?: () => Response;
};

function stub(opts: StubOptions = {}) {
  return vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (input, init) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url === "/api/connections") return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", enabled: true, has_credential: true }]);
    if (url === "/api/connections/c1/schema") return json(schema);
    if (url === "/api/connections/c1/browse") return json({ rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill" } }, { id: { resource: "entities", key: "e2" }, cells: { name: "Hammer" } }], next_cursor: null, has_more: false, count: 2 });
    if (url === "/api/connections/c1/materialize") return json([
      { source: { resource: "entities", key: "e1" }, data: { name: "Drill" } },
      { source: { resource: "entities", key: "e2" }, data: { name: "Hammer" } },
    ]);
    if (url === "/api/templates") return json({ templates: [{ id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300, format: { type: "single" } }] });
    if (url === "/api/templates/tpl") return json(templateDetail);
    if (url === "/api/printers") return json([]);
    if (url.startsWith("/api/render/label") && method === "POST") {
      if (opts.renderLabel) return opts.renderLabel();
      return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
    }
    if (url === "/api/batch" && method === "POST") return new Response(new Blob(["%PDF"]), { status: 200, headers: { "content-type": "application/pdf", "content-disposition": 'attachment; filename="tpl.zip"' } });
    throw new Error(`unexpected fetch: ${url} ${method}`);
  });
}

function renderConnect() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter><Connect /></MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

let fetchMock: ReturnType<typeof stub>;
const countCalls = (path: string) => fetchMock.mock.calls.filter(([u]) => String(u).startsWith(path)).length;

async function browseSelectMaterialize() {
  await screen.findByRole("option", { name: "Home" });
  fireEvent.change(await screen.findByLabelText(/connection/i), { target: { value: "c1" } });
  fireEvent.change(await screen.findByLabelText(/template/i), { target: { value: "tpl" } });
  // Select two rows so we can test row switching.
  fireEvent.click(await screen.findByLabelText("select entities:e1"));
  fireEvent.click(await screen.findByLabelText("select entities:e2"));
  fireEvent.click(await screen.findByRole("button", { name: /add .* row/i }));
  await screen.findByRole("grid", { name: /label rows/i });
}

describe("Connect", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:preview");
    vi.spyOn(URL, "revokeObjectURL").mockReturnValue(undefined);
    fetchMock = stub();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

  it("browses, selects, maps, materializes rows into the grid", async () => {
    renderConnect();
    await browseSelectMaterialize();
    const grid = screen.getByRole("grid", { name: /label rows/i });
    expect(within(grid).getByText("Drill")).toBeInTheDocument();
  });

  it("renders a preview for the selected row and keeps actions enabled on preview error", async () => {
    let renderCallCount = 0;
    fetchMock = stub({
      renderLabel: () => {
        renderCallCount += 1;
        if (renderCallCount === 1) {
          return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
        }
        return new Response(JSON.stringify({ error: { code: "RenderError", message: "bad row" } }), {
          status: 422,
          headers: { "content-type": "application/json" },
        });
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await browseSelectMaterialize();

    // Default selection is the first valid row, so a render/label call fires immediately.
    await waitFor(() => expect(countCalls("/api/render/label")).toBeGreaterThan(0));

    // Select row 2 -> another render fires (which will error per our stub).
    const before = countCalls("/api/render/label");
    fireEvent.click(screen.getByLabelText("preview row 2"));
    await waitFor(() => expect(countCalls("/api/render/label")).toBe(before + 1));

    // Download stays enabled even though the preview endpoint errored.
    expect(screen.getByRole("button", { name: /download/i })).not.toBeDisabled();
  });
});

// #209: the connector grid applies the same datetime cell rule as the CSV grid. A materialized row
// leaves the parameter blank, which is valid; an edited cell that cannot be parsed blocks the run.
describe("Connect: datetime parameters", () => {
  const dtDetail = {
    ...templateDetail,
    params: { printed_on: { type: "datetime", description: "Print date" } },
    layout: [{ type: "text", value: "{name} {printed_on.short_date}" }],
  };

  // The datetime template, plus a connector that offers a `printed_on` field so the default mapping
  // carries `value` into every materialized row.
  const withPrintedOn = (value?: string) => {
    const base = stub();
    return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === "/api/templates/tpl") return json(dtDetail);
      if (url === "/api/connections/c1/schema")
        return json({
          ...schema,
          resources: [
            {
              ...schema.resources[0],
              columns: [
                ...schema.resources[0].columns,
                { key: "printed_on", label: "Printed", ty: "text", tier: "cheap" },
              ],
            },
          ],
        });
      if (url === "/api/connections/c1/materialize")
        return json([
          { source: { resource: "entities", key: "e1" }, data: { name: "Drill", ...(value !== undefined ? { printed_on: value } : {}) } },
          { source: { resource: "entities", key: "e2" }, data: { name: "Hammer", ...(value !== undefined ? { printed_on: value } : {}) } },
        ]);
      return base(input, init);
    }) as ReturnType<typeof stub>;
  };

  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:preview");
    vi.spyOn(URL, "revokeObjectURL").mockReturnValue(undefined);
    fetchMock = withPrintedOn();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

  it("materializes rows with a blank datetime and leaves the run enabled", async () => {
    renderConnect();
    await browseSelectMaterialize();
    expect(screen.getByRole("button", { name: /download/i })).not.toBeDisabled();
  });

  // The value arrives the way a connector row's values actually arrive, through materialize and the
  // field mapping, rather than by driving react-data-grid's editor: the editor is LabelGrid's
  // contract and is covered there. What this asserts is Connect's own validateRow, which is the part
  // #209 changed.
  it("blocks the run when a materialized datetime value cannot be parsed", async () => {
    fetchMock = withPrintedOn("not a date");
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await browseSelectMaterialize();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /download/i })).toBeDisabled(),
    );
  });

  it("leaves the run enabled when the materialized datetime value parses", async () => {
    fetchMock = withPrintedOn("2026-08-19");
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await browseSelectMaterialize();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /download/i })).not.toBeDisabled(),
    );
  });
});
