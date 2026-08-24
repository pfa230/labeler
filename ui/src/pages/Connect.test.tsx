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
    params: { printed_on: { type: "datetime", description: "Print date" } },
    layout: [{ type: "text", value: "{name} {printed_on.short_date}" }],
  };

  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:preview");
    vi.spyOn(URL, "revokeObjectURL").mockReturnValue(undefined);
    const base = stub();
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === "/api/templates/tpl") return json(dtDetail);
      return base(input, init);
    }) as ReturnType<typeof stub>;
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

  // The grid renders the datetime column as a plain text cell, so an edit goes through the same
  // double-click editor every other data field uses.
  // The first double-click can land before the grid has finished wiring its cell handlers after the
  // materialized rows commit, and is then swallowed with no editor opened. Retry it until the editor
  // is up: re-dispatching on an already-open editor is a no-op, so this waits without masking a real
  // failure to open.
  const editPrintedOn = async (value: string) => {
    const grid = screen.getByRole("grid", { name: /label rows/i });
    const input = await waitFor(() => {
      fireEvent.doubleClick(within(grid).getAllByRole("gridcell")[2]);
      return screen.getByLabelText("edit printed_on") as HTMLInputElement;
    });
    fireEvent.change(input, { target: { value } });
    fireEvent.blur(input);
  };

  it("materializes rows with a blank datetime and leaves the run enabled", async () => {
    renderConnect();
    await browseSelectMaterialize();
    expect(screen.getByRole("button", { name: /download/i })).not.toBeDisabled();
  });

  it("blocks the run when a datetime cell cannot be parsed, and unblocks when corrected", async () => {
    renderConnect();
    await browseSelectMaterialize();

    await editPrintedOn("not a date");
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /download/i })).toBeDisabled(),
    );

    await editPrintedOn("2026-08-19");
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /download/i })).not.toBeDisabled(),
    );
  });
});
