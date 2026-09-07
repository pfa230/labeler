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

type StubConnection = {
  id: string;
  connector: string;
  name: string;
  base_url: string;
  public_url?: string | null;
  enabled: boolean;
  has_credential: boolean;
  transforms?: Array<{ resource: string; source: string; pattern: string; target?: string }>;
};

type StubOptions = {
  renderLabel?: () => Response;
  connections?: StubConnection[];
  connectionsError?: boolean;
  settings?: Record<string, { value: unknown; is_default: boolean }>;
  settingsError?: boolean;
};

function stub(opts: StubOptions = {}) {
  let state: StubConnection[] = opts.connections
    ? [...opts.connections]
    : [
        {
          id: "c1",
          connector: "homebox",
          name: "Home",
          base_url: "http://hb",
          enabled: true,
          has_credential: true,
        },
      ];
  let connectionsError = opts.connectionsError ?? false;
  let currentSettings = opts.settings ?? {
    default_connection_id: { value: null, is_default: true },
  };
  let settingsError = opts.settingsError ?? false;

  const fn = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (input, init) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.includes("/inputs")) {
      const parsedBody = init?.body ? JSON.parse(String(init.body)) : { labels: [] };
      const labels = parsedBody.labels ?? [{ data: {} }];
      return json({ inputs: labels.map(() => [{ name: "name", control: "text" }]) });
    }
    if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(schema);
    if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({ rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill" } }, { id: { resource: "entities", key: "e2" }, cells: { name: "Hammer" } }], next_cursor: null, has_more: false, count: 2 });
    if (url.startsWith("/api/connections/") && url.endsWith("/materialize")) {
      const parsed = init?.body ? JSON.parse(String(init.body)) : null;
      const requestedRows: Array<{ resource: string; key: string }> = parsed?.rows ?? [
        { resource: "entities", key: "e1" },
        { resource: "entities", key: "e2" },
      ];
      const allData: Record<string, string> = { e1: "Drill", e2: "Hammer" };
      return json(
        requestedRows.map((r) => ({
          source: { resource: r.resource, key: r.key },
          data: { name: allData[r.key] ?? r.key },
        })),
      );
    }
    if (url.startsWith("/api/connections/") && method === "DELETE") {
      const id = decodeURIComponent(url.slice("/api/connections/".length));
      state = state.filter((c) => c.id !== id);
      if (currentSettings.default_connection_id?.value === id) {
        currentSettings = {
          ...currentSettings,
          default_connection_id: { value: null, is_default: true },
        };
      }
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/connections/") && method === "PUT") {
      const id = decodeURIComponent(url.slice("/api/connections/".length));
      const b = JSON.parse(init!.body as string);
      state = state.map((c) =>
        c.id === id
          ? {
              ...c,
              name: b.name,
              base_url: b.base_url,
              public_url: "public_url" in b ? b.public_url : c.public_url,
              enabled: b.enabled !== undefined ? b.enabled : c.enabled,
              has_credential: c.has_credential || !!b.credential,
              transforms: b.transforms ?? c.transforms,
            }
          : c,
      );
      return json(state.find((c) => c.id === id)!);
    }
    if (url === "/api/connections" && method === "POST") {
      const b = JSON.parse(init!.body as string);
      const c: StubConnection = {
        id: b.id ?? `c_${state.length + 1}`,
        connector: b.connector,
        name: b.name,
        base_url: b.base_url,
        public_url: b.public_url ?? null,
        enabled: b.enabled ?? true,
        has_credential: !!b.credential,
        transforms: b.transforms ?? [],
      };
      state = [...state, c];
      return json(c, 201);
    }
    if (url.startsWith("/api/connections") && (url === "/api/connections" || url.startsWith("/api/connections?"))) {
      if (connectionsError) return json({ error: "Failed" }, 500);
      return json(state);
    }
    if (url === "/api/settings" && method === "GET") {
      if (settingsError) return json({ error: "Failed" }, 500);
      return json(currentSettings);
    }
    if (url === "/api/settings/default_connection_id" && method === "PUT") {
      const b = JSON.parse(init!.body as string);
      currentSettings = {
        ...currentSettings,
        default_connection_id: { value: b.value, is_default: false },
      };
      return json({ value: b.value, is_default: false });
    }
    if (url === "/api/settings/default_connection_id" && method === "DELETE") {
      currentSettings = {
        ...currentSettings,
        default_connection_id: { value: null, is_default: true },
      };
      return new Response(null, { status: 204 });
    }
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
    setSettingsError: (err: boolean) => { settingsError = err; },
    setConnections: (conns: StubConnection[]) => { state = conns; },
    setConnectionsError: (err: boolean) => { connectionsError = err; },
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
  fireEvent.change(await screen.findByLabelText(/^connection$/i), { target: { value: "c1" } });
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
    const select = await screen.findByLabelText(/^connection$/i);
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
    const select = await screen.findByLabelText(/^connection$/i);
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
    const select = await screen.findByLabelText(/^connection$/i);
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
    const select = await screen.findByLabelText(/^connection$/i);
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
    const select = await screen.findByLabelText(/^connection$/i);
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe(""));
    expect(screen.queryByLabelText(/template/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("grid")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /manage connections/i })).toBeInTheDocument();
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
    const select = await screen.findByLabelText(/^connection$/i);
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
    const select = await screen.findByLabelText(/^connection$/i);
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
    const select = await screen.findByLabelText(/^connection$/i);
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
    const select = await screen.findByLabelText(/^connection$/i);
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

  it("starts collapsed when list has connections, starts expanded when empty, and starts collapsed on failure", async () => {
    // 1. Loaded with connection -> collapsed, opens on click
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", enabled: true, has_credential: true },
      ],
    });
    vi.stubGlobal("fetch", fetchMock);
    const { unmount } = renderConnect();
    const picker = await screen.findByLabelText(/^connection$/i);
    await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));
    const btn = await screen.findByRole("button", { name: /manage connections/i });
    expect(btn).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("heading", { name: /^connections$/i })).not.toBeInTheDocument();

    fireEvent.click(btn);
    expect(btn).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("heading", { name: /^connections$/i })).toBeInTheDocument();
    unmount();

    // 2. Loaded empty -> starts expanded
    fetchMock = stub({ connections: [] });
    vi.stubGlobal("fetch", fetchMock);
    const { unmount: unmountEmpty } = renderConnect();
    const btnEmpty = await screen.findByRole("button", { name: /manage connections/i });
    await waitFor(() => expect(btnEmpty).toHaveAttribute("aria-expanded", "true"));
    expect(await screen.findByText(/no connections configured/i)).toBeInTheDocument();
    unmountEmpty();

    // 3. Failed -> starts collapsed
    fetchMock = stub({ connectionsError: true });
    vi.stubGlobal("fetch", fetchMock);
    const { queryClient: qcFailed } = renderConnect();
    const btnFailed = await screen.findByRole("button", { name: /manage connections/i });
    await waitFor(() => expect(qcFailed.getQueryState(["connections"])?.status).toBe("error"));
    expect(btnFailed).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText(/failed to load connections/i)).not.toBeInTheDocument();

    fireEvent.click(btnFailed);
    expect(btnFailed).toHaveAttribute("aria-expanded", "true");
    expect(await screen.findByText(/failed to load connections/i)).toBeInTheDocument();
  });

  it("adding an enabled connection in the block puts it in the picker and leaves selection, browse table and row selection unchanged", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true },
      ],
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await screen.findByRole("option", { name: "Home 1" });
    const picker = await screen.findByLabelText(/^connection$/i);
    await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));

    // Select row
    const checkbox = await screen.findByLabelText("select entities:e1");
    fireEvent.click(checkbox);
    await waitFor(() => expect(screen.getByLabelText("select entities:e1")).toBeChecked());

    // Open block and add enabled connection
    fireEvent.click(await screen.findByRole("button", { name: /manage connections/i }));
    fireEvent.click(screen.getByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home 2" } });
    fireEvent.change(screen.getByLabelText(/^base url$/i), { target: { value: "http://hb2" } });
    fireEvent.change(screen.getByLabelText(/^api key/i), { target: { value: "secret" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

    // Verify "Home 2" is in the picker without reload
    await screen.findByRole("option", { name: "Home 2" });
    expect((picker as HTMLSelectElement).value).toBe("c1");
    expect(screen.getByLabelText("select entities:e1")).toBeChecked();
  });

  it("adding a disabled connection lists it in block table, does not offer it in picker, and leaves selection unchanged", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true },
      ],
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await screen.findByRole("option", { name: "Home 1" });
    const picker = await screen.findByLabelText(/^connection$/i);
    await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));

    // Open block and add disabled connection
    fireEvent.click(await screen.findByRole("button", { name: /manage connections/i }));
    fireEvent.click(screen.getByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home 2" } });
    fireEvent.change(screen.getByLabelText(/^base url$/i), { target: { value: "http://hb2" } });
    fireEvent.change(screen.getByLabelText(/^api key/i), { target: { value: "secret" } });
    fireEvent.click(screen.getByLabelText(/^enabled$/i));
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

    // Wait for table row to appear in Connections table
    await screen.findByText("Home 2");
    // Picker should not offer Home 2
    expect(screen.queryByRole("option", { name: "Home 2" })).not.toBeInTheDocument();
    expect((picker as HTMLSelectElement).value).toBe("c1");
  });

  it("renaming the selected connection offers it under the new name without reload and leaves it selected", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true },
      ],
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    const picker = await screen.findByLabelText(/^connection$/i);
    await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));

    // Open block and edit name
    fireEvent.click(await screen.findByRole("button", { name: /manage connections/i }));
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home Renamed" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

    // Picker offers "Home Renamed" and is still selected
    await screen.findByRole("option", { name: "Home Renamed" });
    expect((picker as HTMLSelectElement).value).toBe("c1");
  });

  it("deleting the currently selected connection returns picker to choose a connection and leaves no browse table, row selection or composer", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", enabled: true, has_credential: true },
      ],
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await browseSelectMaterialize();
    const picker = await screen.findByLabelText(/^connection$/i);
    expect((picker as HTMLSelectElement).value).toBe("c1");
    expect(screen.getByRole("grid", { name: /label rows/i })).toBeInTheDocument();

    // Open block and delete c1
    fireEvent.click(await screen.findByRole("button", { name: /manage connections/i }));
    fireEvent.click(screen.getByRole("button", { name: /^delete$/i }));
    fireEvent.click(screen.getByRole("button", { name: /^confirm$/i }));

    // Returns picker to "choose a connection"
    await waitFor(() => expect((picker as HTMLSelectElement).value).toBe(""));
    expect(screen.queryByRole("grid", { name: /label rows/i })).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/template/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText("select entities:e1")).not.toBeInTheDocument();
  });

  it("disabling the currently selected connection returns picker to choose a connection and leaves no browse table, row selection or composer", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", enabled: true, has_credential: true },
      ],
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await browseSelectMaterialize();
    const picker = await screen.findByLabelText(/^connection$/i);
    expect((picker as HTMLSelectElement).value).toBe("c1");
    expect(screen.getByRole("grid", { name: /label rows/i })).toBeInTheDocument();

    // Open block and edit c1 to disable it
    fireEvent.click(await screen.findByRole("button", { name: /manage connections/i }));
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    fireEvent.click(screen.getByLabelText(/^enabled$/i));
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

    // Returns picker to "choose a connection"
    await waitFor(() => expect((picker as HTMLSelectElement).value).toBe(""));
    expect(screen.queryByRole("grid", { name: /label rows/i })).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/template/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText("select entities:e1")).not.toBeInTheDocument();
  });

  it("rows selected against a connection do not come back when connection is cleared and another is picked", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true },
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", enabled: true, has_credential: true },
      ],
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    const picker = await screen.findByLabelText(/^connection$/i);
    await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));

    // Select two rows on c1
    fireEvent.click(await screen.findByLabelText("select entities:e1"));
    fireEvent.click(await screen.findByLabelText("select entities:e2"));
    expect(screen.getByLabelText("select entities:e1")).toBeChecked();
    expect(screen.getByLabelText("select entities:e2")).toBeChecked();

    // Delete c1 in Manage connections
    fireEvent.click(await screen.findByRole("button", { name: /manage connections/i }));
    const withinSection = within(screen.getByRole("heading", { name: /^connections$/i }).closest("section")!);
    const editBtns = withinSection.getAllByRole("button", { name: /^delete$/i });
    fireEvent.click(editBtns[0]);
    fireEvent.click(withinSection.getByRole("button", { name: /^confirm$/i }));

    await waitFor(() => expect((picker as HTMLSelectElement).value).toBe(""));

    // Pick c2
    fireEvent.change(picker, { target: { value: "c2" } });
    await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c2"));

    // Verify nothing is selected for c2
    const e1 = await screen.findByLabelText("select entities:e1");
    expect(e1).not.toBeChecked();
    expect(screen.getByLabelText("select entities:e2")).not.toBeChecked();

    // Pick template and select 1 row on c2
    fireEvent.change(await screen.findByLabelText(/template/i), { target: { value: "tpl" } });
    fireEvent.click(e1);
    const addBtn = await screen.findByRole("button", { name: /add .* row/i });
    expect(addBtn).toHaveTextContent(/add 1 row/i);
    fireEvent.click(addBtn);

    const grid = await screen.findByRole("grid", { name: /label rows/i });
    expect(within(grid).getByText("Drill")).toBeInTheDocument();
    expect(within(grid).queryByText("Hammer")).not.toBeInTheDocument();
  });

  it("leaves selected connection, browse table and row selection unchanged when a later connections request fails", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true },
      ],
    });
    vi.stubGlobal("fetch", fetchMock);

    const { queryClient } = renderConnect();
    const picker = await screen.findByLabelText(/^connection$/i);
    await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));

    // Select row
    const checkbox = await screen.findByLabelText("select entities:e1");
    fireEvent.click(checkbox);
    await waitFor(() => expect(screen.getByLabelText("select entities:e1")).toBeChecked());

    // Later connections request fails
    fetchMock.setConnectionsError(true);
    await queryClient.invalidateQueries({ queryKey: ["connections"] });

    // Selection and checked row remain
    await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));
    expect(screen.getByLabelText("select entities:e1")).toBeChecked();
  });

  it("naming a different connection as the default while working on one leaves selected connection, browse table and row selection unchanged", async () => {
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
    const picker = await screen.findByLabelText(/^connection$/i);
    await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));

    // Select row
    const checkbox = await screen.findByLabelText("select entities:e1");
    fireEvent.click(checkbox);
    await waitFor(() => expect(screen.getByLabelText("select entities:e1")).toBeChecked());

    // Open block and name c2 as default connection
    fireEvent.click(screen.getByRole("button", { name: /manage connections/i }));
    const defaultSelect = await screen.findByLabelText(/^default connection$/i);
    fireEvent.change(defaultSelect, { target: { value: "c2" } });

    // Selected connection in main picker and row selection are unchanged
    await waitFor(() => expect((defaultSelect as HTMLSelectElement).value).toBe("c2"));
    expect((picker as HTMLSelectElement).value).toBe("c1");
    expect(screen.getByLabelText("select entities:e1")).toBeChecked();
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
    fireEvent.change(await screen.findByLabelText(/^connection$/i), { target: { value: "c1" } });
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
    fireEvent.change(await screen.findByLabelText(/^connection$/i), { target: { value: "c1" } });
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
    fireEvent.change(await screen.findByLabelText(/^connection$/i), { target: { value: "c1" } });
    fireEvent.change(await screen.findByLabelText(/template/i), { target: { value: "tpl" } });
    fireEvent.click(await screen.findByLabelText("select entities:e1"));
    fireEvent.click(await screen.findByRole("button", { name: /add .* row/i }));

    const grid = await screen.findByRole("grid", { name: /label rows/i });
    expect(within(grid).getByText("Drill")).toBeInTheDocument();
    expect(screen.getByLabelText("map tags")).toHaveValue("");
    expect(screen.getByRole("button", { name: /download/i })).toBeEnabled();
  });
});
