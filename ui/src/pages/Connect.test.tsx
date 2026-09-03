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
    columns: [{ key: "name", label: "Name", ty: "text", tier: "cheap", multi_valued: false }], filters: [] }],
  relationships: [],
};
const templateDetail = {
  id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300,
  format: { type: "single" },
  inputs: {
    all: [{ name: "name", control: "text" }],
    default: [{ name: "name", control: "text" }],
  },
};

type StubOptions = {
  renderLabel?: () => Response;
  connections?: Array<{ id: string; connector: string; name: string; base_url: string; enabled: boolean; has_credential: boolean }>;
  connectionsError?: boolean;
  settings?: Record<string, { value: unknown; is_default: boolean }>;
  settingsError?: boolean;
};

function stub(opts: StubOptions = {}) {
  let currentSettings = opts.settings ?? {
    default_connection_id: { value: null, is_default: true },
  };

  const fn = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (input, init) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.includes("/inputs")) {
      const parsedBody = init?.body ? JSON.parse(String(init.body)) : { labels: [] };
      const labels = parsedBody.labels ?? [{ data: {} }];
      return json({ inputs: labels.map(() => [{ name: "name", control: "text" }]) });
    }
    if (url === "/api/connections") {
      if (opts.connectionsError) return json({ error: "Failed" }, 500);
      return json(opts.connections ?? [{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", enabled: true, has_credential: true }]);
    }
    if (url === "/api/settings") {
      if (opts.settingsError) return json({ error: "Failed" }, 500);
      return json(currentSettings);
    }
    if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(schema);
    if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({ rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill" } }, { id: { resource: "entities", key: "e2" }, cells: { name: "Hammer" } }], next_cursor: null, has_more: false, count: 2 });
    if (url.startsWith("/api/connections/") && url.endsWith("/materialize")) return json([
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

  return Object.assign(fn, {
    setSettings: (s: Record<string, { value: unknown; is_default: boolean }>) => { currentSettings = s; },
  });
}

function renderConnect(client?: QueryClient) {
  const qc = client ?? new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter><Connect /></MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
  return { ...view, queryClient: qc };
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

  it("selects the stored default connection on open and loads its browse rows without a click", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true },
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", enabled: true, has_credential: true },
      ],
      settings: {
        default_connection_id: { value: "c2", is_default: false },
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    const select = await screen.findByLabelText(/connection/i);
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe("c2"));
    await waitFor(() => expect(countCalls("/api/connections/c2/browse")).toBeGreaterThan(0));
  });

  it("falls back to the first enabled connection when no default is stored", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true },
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", enabled: true, has_credential: true },
      ],
      settings: {
        default_connection_id: { value: null, is_default: true },
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    const select = await screen.findByLabelText(/connection/i);
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe("c1"));
  });

  it("falls back to the first enabled connection when the stored default is disabled", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: false, has_credential: true },
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", enabled: true, has_credential: true },
      ],
      settings: {
        default_connection_id: { value: "c1", is_default: false },
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    const select = await screen.findByLabelText(/connection/i);
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe("c2"));
  });

  it("falls back to the first enabled connection when the stored default names no connection", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true },
      ],
      settings: {
        default_connection_id: { value: "nonexistent", is_default: false },
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    const select = await screen.findByLabelText(/connection/i);
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe("c1"));
  });

  it("selects nothing when no connection is enabled", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: false, has_credential: true },
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", enabled: false, has_credential: true },
      ],
      settings: {
        default_connection_id: { value: null, is_default: true },
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    const select = await screen.findByLabelText(/connection/i);
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe(""));
    expect(screen.queryByLabelText(/template/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("grid")).not.toBeInTheDocument();
  });

  it("falls back to first enabled connection when settings query errors", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true },
      ],
      settingsError: true,
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    const select = await screen.findByLabelText(/connection/i);
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe("c1"));
  });

  it("resolves equal-name connections in list (id) order", async () => {
    fetchMock = stub({
      connections: [
        { id: "a", connector: "homebox", name: "Home", base_url: "http://hba", enabled: true, has_credential: true },
        { id: "b", connector: "homebox", name: "Home", base_url: "http://hbb", enabled: true, has_credential: true },
      ],
      settings: {
        default_connection_id: { value: null, is_default: true },
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    const select = await screen.findByLabelText(/connection/i);
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe("a"));
  });

  it("does not move the selection or drop row selection when settings query refetches with a new default", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true },
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", enabled: true, has_credential: true },
      ],
      settings: {
        default_connection_id: { value: "c1", is_default: false },
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    const { queryClient } = renderConnect();
    const select = await screen.findByLabelText(/connection/i);
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe("c1"));

    // Select a row in the browser
    const checkbox = await screen.findByLabelText("select entities:e1");
    fireEvent.click(checkbox);
    await waitFor(() => expect(screen.getByLabelText("select entities:e1")).toBeChecked());

    // Stored setting changes in background to c2
    fetchMock.setSettings({
      default_connection_id: { value: "c2", is_default: false },
    });
    await queryClient.invalidateQueries({ queryKey: ["settings"] });

    // Selection remains c1 and selected row checkbox remains checked
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe("c1"));
    expect(screen.getByLabelText("select entities:e1")).toBeChecked();
  });

  it("clears row selection and writes no setting on manual pick", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true },
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", enabled: true, has_credential: true },
      ],
      settings: {
        default_connection_id: { value: "c1", is_default: false },
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    const select = await screen.findByLabelText(/connection/i);
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe("c1"));

    // Select a row
    const checkbox = await screen.findByLabelText("select entities:e1");
    fireEvent.click(checkbox);
    await waitFor(() => expect(screen.getByLabelText("select entities:e1")).toBeChecked());

    // Manually switch connection to c2
    fireEvent.change(select, { target: { value: "c2" } });
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe("c2"));

    // Wait for new connection browser to mount and verify checkbox is unchecked
    await waitFor(() => expect(screen.getByLabelText("select entities:e1")).not.toBeChecked());

    // Verify no settings mutation (PUT/POST/DELETE to /api/settings) was made
    const settingsMutations = fetchMock.mock.calls.filter(([u, init]) => {
      const url = String(u);
      const method = (init?.method ?? "GET").toUpperCase();
      return url.startsWith("/api/settings") && method !== "GET";
    });
    expect(settingsMutations).toHaveLength(0);
  });
});

// #209: the connector grid applies the same datetime cell rule as the CSV grid. A materialized row
// leaves the parameter blank, which is valid; an edited cell that cannot be parsed blocks the run.
describe("Connect: datetime parameters", () => {
  const dtDetail = {
    ...templateDetail,
    inputs: {
      all: [
        { name: "name", control: "text" as const },
        { name: "printed_on", control: "datetime" as const, description: "Print date" },
      ],
      default: [
        { name: "name", control: "text" as const },
        { name: "printed_on", control: "datetime" as const, description: "Print date" },
      ],
    },
    layout: [{ type: "text", value: "{name} {printed_on.short_date}" }],
  };

  // The datetime template, plus a connector that offers a `printed_on` field so the default mapping
  // carries `value` into every materialized row.
  const withPrintedOn = (value?: string, required = false) => {
    const base = stub();
    return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.includes("/inputs")) {
        const parsedBody = init?.body ? JSON.parse(String(init.body)) : { labels: [] };
        const labels = parsedBody.labels ?? [{ data: {} }];
        return json({
          inputs: labels.map(() => [
            { name: "name", control: "text" },
            { name: "printed_on", control: "datetime", required, description: "Print date" },
          ]),
        });
      }
      const tDetail = {
        ...dtDetail,
        inputs: {
          all: [
            { name: "name", control: "text" as const },
            { name: "printed_on", control: "datetime" as const, required, description: "Print date" },
          ],
          default: [
            { name: "name", control: "text" as const },
            { name: "printed_on", control: "datetime" as const, required, description: "Print date" },
          ],
        },
      };
      if (url === "/api/templates/tpl") return json(tDetail);
      if (url === "/api/connections/c1/schema")
        return json({
          ...schema,
          resources: [
            {
              ...schema.resources[0],
              columns: [
                ...schema.resources[0].columns,
                { key: "printed_on", label: "Printed", ty: "text", tier: "cheap", multi_valued: false },
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

  it("blocks the run when a blank datetime is materialized for a required parameter", async () => {
    fetchMock = withPrintedOn(undefined, true);
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await browseSelectMaterialize();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /download/i })).toBeDisabled(),
    );
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

  it("surfaces default_error.message for a required param whose default is broken", async () => {
    fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.includes("/inputs")) {
        return json({
          inputs: [
            [
              {
                name: "name",
                control: "text",
                required: true,
                default_error: {
                  reason: "param_default_unresolvable",
                  message: "vars.missing not found",
                  token: "vars.missing",
                },
              },
            ],
          ],
        });
      }
      if (url === "/api/templates/tpl") {
        return json({
          ...templateDetail,
          inputs: {
            all: [{ name: "name", control: "text", required: true, default_error: { reason: "param_default_unresolvable", message: "vars.missing not found", token: "vars.missing" } }],
            default: [{ name: "name", control: "text", required: true, default_error: { reason: "param_default_unresolvable", message: "vars.missing not found", token: "vars.missing" } }],
          },
        });
      }
      if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(schema);
      if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({ rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill" } }, { id: { resource: "entities", key: "e2" }, cells: { name: "Hammer" } }], next_cursor: null, has_more: false, count: 2 });
      if (url.startsWith("/api/connections/") && url.endsWith("/materialize")) return json([{ source: { resource: "entities", key: "e1" }, data: { name: "" } }, { source: { resource: "entities", key: "e2" }, data: { name: "" } }]);
      if (url === "/api/templates") return json({ templates: [{ id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300, format: { type: "single" } }] });
      if (url === "/api/connections") return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", enabled: true, has_credential: true }]);
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      if (url === "/api/printers") return json([]);
      if (url.startsWith("/api/render/label")) return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url === "/api/batch") return new Response(new Blob(["%PDF"]), { status: 200, headers: { "content-type": "application/pdf" } });
      throw new Error(`unexpected fetch: ${url}`);
    }) as ReturnType<typeof stub>;
    vi.stubGlobal("fetch", fetchMock);
    renderConnect();
    await browseSelectMaterialize();
    expect((await screen.findAllByText(/vars\.missing/)).length).toBe(2);
    expect(screen.getByRole("button", { name: /download/i })).toBeDisabled();
  });

  it("refuses mapping multi-valued column to scalar parameter and scalar column to list parameter, showing refusal naming both and adding no rows", async () => {
    const multiValuedSchema = {
      version: "homebox-1",
      resources: [{
        id: "entities",
        label: "Items",
        view: "table",
        columns: [
          { key: "name", label: "Name", ty: "text", tier: "cheap", multi_valued: false },
          { key: "tags", label: "Tags", ty: "text", tier: "cheap", multi_valued: true },
        ],
        filters: [],
      }],
      relationships: [],
    };

    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const method = (init?.method ?? "GET").toUpperCase();
      if (url.includes("/inputs")) {
        const parsedBody = init?.body ? JSON.parse(String(init.body)) : { labels: [] };
        const labels = parsedBody.labels ?? [{ data: {} }];
        return json({
          inputs: labels.map(() => [
            { name: "title", control: "text" },
            { name: "tagList", control: "list" },
          ]),
        });
      }
      if (url === "/api/connections") return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", enabled: true, has_credential: true }]);
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(multiValuedSchema);
      if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({ rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill", tags: ["KIDS"] } }], next_cursor: null, has_more: false, count: 1 });
      if (url.startsWith("/api/connections/") && url.endsWith("/materialize")) return json([{ source: { resource: "entities", key: "e1" }, data: { name: "Drill", tags: ["KIDS"] } }]);
      if (url === "/api/templates") return json({ templates: [{ id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300, format: { type: "single" } }] });
      if (url === "/api/templates/tpl") {
        return json({
          ...templateDetail,
          inputs: {
            all: [
              { name: "title", control: "text" },
              { name: "tagList", control: "list" },
            ],
            default: [
              { name: "title", control: "text" },
              { name: "tagList", control: "list" },
            ],
          },
        });
      }
      if (url === "/api/printers") return json([]);
      if (url.startsWith("/api/render/label") && method === "POST") return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url === "/api/batch" && method === "POST") return new Response(new Blob(["%PDF"]), { status: 200, headers: { "content-type": "application/pdf" } });
      throw new Error(`unexpected fetch: ${url} ${method}`);
    }) as ReturnType<typeof stub>;
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await screen.findByRole("option", { name: "Home" });
    fireEvent.change(await screen.findByLabelText(/connection/i), { target: { value: "c1" } });
    fireEvent.change(await screen.findByLabelText(/template/i), { target: { value: "tpl" } });
    fireEvent.click(await screen.findByLabelText("select entities:e1"));

    // Case 1: Map multi-valued column 'tags' to scalar parameter 'title'
    fireEvent.change(await screen.findByLabelText("map title"), { target: { value: "tags" } });
    expect(await screen.findByText(/tags.*title|title.*tags/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /add .* row/i })).toBeDisabled();
    expect(screen.queryByRole("grid", { name: /label rows/i })).toBeNull();

    // Reset title mapping
    fireEvent.change(screen.getByLabelText("map title"), { target: { value: "" } });

    // Case 2: Map scalar column 'name' to list parameter 'tagList'
    fireEvent.change(screen.getByLabelText("map tagList"), { target: { value: "name" } });
    expect(await screen.findByText(/name.*tagList|tagList.*name/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /add .* row/i })).toBeDisabled();
    expect(screen.queryByRole("grid", { name: /label rows/i })).toBeNull();

    // Case 3: Correct mapping (scalar -> scalar, list -> list)
    fireEvent.change(screen.getByLabelText("map title"), { target: { value: "name" } });
    fireEvent.change(screen.getByLabelText("map tagList"), { target: { value: "tags" } });
    expect(screen.queryByText(/cannot map/i)).toBeNull();
    const addButton = screen.getByRole("button", { name: /add .* row/i });
    expect(addButton).toBeEnabled();
    fireEvent.click(addButton);

    const grid = await screen.findByRole("grid", { name: /label rows/i });
    expect(within(grid).getByText("Drill")).toBeInTheDocument();
    expect(within(grid).getByText("KIDS")).toBeInTheDocument();
  });

  it("mapping multi-valued tags column to list parameter and adding rows sends batch with array data and empty array for untagged item", async () => {
    const multiValuedSchema = {
      version: "homebox-1",
      resources: [{
        id: "entities",
        label: "Items",
        view: "table",
        columns: [
          { key: "name", label: "Name", ty: "text", tier: "cheap", multi_valued: false },
          { key: "tags", label: "Tags", ty: "text", tier: "cheap", multi_valued: true },
        ],
        filters: [],
      }],
      relationships: [],
    };

    let submittedBatch: { labels: Array<{ data: { tags?: string[] } }> } | null = null;

    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const method = (init?.method ?? "GET").toUpperCase();
      if (url.includes("/inputs")) {
        const parsedBody = init?.body ? JSON.parse(String(init.body)) : { labels: [] };
        const labels = parsedBody.labels ?? [{ data: {} }];
        return json({
          inputs: labels.map(() => [
            { name: "name", control: "text" },
            { name: "tags", control: "list" },
          ]),
        });
      }
      if (url === "/api/connections") return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", enabled: true, has_credential: true }]);
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(multiValuedSchema);
      if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({
        rows: [
          { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", tags: ["KIDS", "CONSUMABLE"] } },
          { id: { resource: "entities", key: "e2" }, cells: { name: "Hammer", tags: [] } },
        ],
        next_cursor: null,
        has_more: false,
        count: 2,
      });
      if (url.startsWith("/api/connections/") && url.endsWith("/materialize")) return json([
        { source: { resource: "entities", key: "e1" }, data: { name: "Drill", tags: ["KIDS", "CONSUMABLE"] } },
        { source: { resource: "entities", key: "e2" }, data: { name: "Hammer", tags: [] } },
      ]);
      if (url === "/api/templates") return json({ templates: [{ id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300, format: { type: "single" } }] });
      if (url === "/api/templates/tpl") {
        return json({
          ...templateDetail,
          inputs: {
            all: [
              { name: "name", control: "text" },
              { name: "tags", control: "list" },
            ],
            default: [
              { name: "name", control: "text" },
              { name: "tags", control: "list" },
            ],
          },
        });
      }
      if (url === "/api/printers") return json([]);
      if (url.startsWith("/api/render/label") && method === "POST") return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url === "/api/batch" && method === "POST") {
        submittedBatch = JSON.parse(String(init?.body));
        return new Response(new Blob(["%PDF"]), { status: 200, headers: { "content-type": "application/pdf" } });
      }
      throw new Error(`unexpected fetch: ${url} ${method}`);
    }) as ReturnType<typeof stub>;
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await screen.findByRole("option", { name: "Home" });
    fireEvent.change(await screen.findByLabelText(/connection/i), { target: { value: "c1" } });
    fireEvent.change(await screen.findByLabelText(/template/i), { target: { value: "tpl" } });

    // Both name and tags parameters appear in field mapping
    expect(await screen.findByLabelText("map name")).toBeInTheDocument();
    expect(await screen.findByLabelText("map tags")).toBeInTheDocument();

    // Select both rows and add
    fireEvent.click(await screen.findByLabelText("select entities:e1"));
    fireEvent.click(await screen.findByLabelText("select entities:e2"));
    fireEvent.click(screen.getByRole("button", { name: /add .* row/i }));

    const grid = await screen.findByRole("grid", { name: /label rows/i });
    expect(within(grid).getByText("KIDS, CONSUMABLE")).toBeInTheDocument();

    // Run batch download
    await waitFor(() => {
      fireEvent.click(screen.getByRole("button", { name: /download/i }));
      expect(submittedBatch).not.toBeNull();
    });
    expect(submittedBatch!.labels).toHaveLength(2);
    expect(submittedBatch!.labels[0].data.tags).toEqual(["KIDS", "CONSUMABLE"]);
    expect(submittedBatch!.labels[1].data.tags).toEqual([]);
  });

  it("leaves grid valid and download button enabled when a required list parameter is left unmapped", async () => {
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const method = (init?.method ?? "GET").toUpperCase();
      if (url.includes("/inputs")) {
        const parsedBody = init?.body ? JSON.parse(String(init.body)) : { labels: [] };
        const labels = parsedBody.labels ?? [{ data: {} }];
        return json({
          inputs: labels.map(() => [
            { name: "name", control: "text" },
            { name: "tags", control: "list", required: true },
          ]),
        });
      }
      if (url === "/api/connections") return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", enabled: true, has_credential: true }]);
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(schema);
      if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({ rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill" } }], next_cursor: null, has_more: false, count: 1 });
      if (url.startsWith("/api/connections/") && url.endsWith("/materialize")) return json([{ source: { resource: "entities", key: "e1" }, data: { name: "Drill" } }]);
      if (url === "/api/templates") return json({ templates: [{ id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300, format: { type: "single" } }] });
      if (url === "/api/templates/tpl") {
        return json({
          ...templateDetail,
          inputs: {
            all: [
              { name: "name", control: "text" },
              { name: "tags", control: "list", required: true },
            ],
            default: [
              { name: "name", control: "text" },
              { name: "tags", control: "list", required: true },
            ],
          },
        });
      }
      if (url === "/api/printers") return json([]);
      if (url.startsWith("/api/render/label") && method === "POST") return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url === "/api/batch" && method === "POST") return new Response(new Blob(["%PDF"]), { status: 200, headers: { "content-type": "application/pdf" } });
      throw new Error(`unexpected fetch: ${url} ${method}`);
    }) as ReturnType<typeof stub>;
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await screen.findByRole("option", { name: "Home" });
    fireEvent.change(await screen.findByLabelText(/connection/i), { target: { value: "c1" } });
    fireEvent.change(await screen.findByLabelText(/template/i), { target: { value: "tpl" } });
    fireEvent.click(await screen.findByLabelText("select entities:e1"));
    fireEvent.click(await screen.findByRole("button", { name: /add .* row/i }));

    const grid = await screen.findByRole("grid", { name: /label rows/i });
    expect(within(grid).getByText("Drill")).toBeInTheDocument();
    expect(screen.getByLabelText("map tags")).toHaveValue("");
    expect(screen.getByRole("button", { name: /download/i })).toBeEnabled();
  });
});
