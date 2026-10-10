import { useEffect } from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Routes, Route, useLocation } from "react-router-dom";
import { ToastProvider } from "../app/toast";
import { Connect } from "./Connect";
import {
  useSaveConnection,
  useDeleteConnection,
  useSetDefaultConnection,
  useClearDefaultConnection,
} from "../api/connectors";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const schema = {
  version: "homebox-1",
  resources: [{ id: "entities", label: "Items", view: "table",
    fields_incomplete: false,
    columns: [{ key: "name", label: "Name", ty: "text", tier: "cheap", multi_valued: false }], filters: [] }],
  relationships: [],
};
const templateDetail = {
  id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300,
  format: { type: "single" },
  params: [{ name: "name", type: "string", control: "text" }],
};

type StubConnection = {
  id: string;
  connector: string;
  name: string;
  base_url: string;
  public_url?: string | null;
  has_credential: boolean;
};

type StubOptions = {
  renderLabel?: () => Response;
  batch?: (body: Record<string, unknown>) => Response;
  connections?: StubConnection[];
  connectionsError?: boolean;
  settings?: Record<string, { value: unknown; is_default: boolean }>;
  settingsError?: boolean;
  templates?: Array<{ id: string; name: string; description: string; unit: string; dpi: number; format: { type: string } }>;
  templateDetail?: unknown;
  templateDetails?: Record<string, unknown>;
  browseRows?: Array<{ id: { resource: string; key: string }; cells: Record<string, string> }>;
  materializeData?: Record<string, string>;
  printers?: Array<{ id: string; name: string }>;
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
    if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(schema);
    if (url.startsWith("/api/connections/") && url.endsWith("/browse")) {
      const rows = opts.browseRows ?? [
        { id: { resource: "entities", key: "e1" }, cells: { name: "Drill" } },
        { id: { resource: "entities", key: "e2" }, cells: { name: "Hammer" } },
      ];
      return json({ rows, has_more: false, count: rows.length });
    }
    if (url.startsWith("/api/connections/") && url.endsWith("/materialize")) {
      const parsed = init?.body ? JSON.parse(String(init.body)) : null;
      const requestedRows: Array<{ resource: string; key: string }> = parsed?.rows ?? [
        { resource: "entities", key: "e1" },
        { resource: "entities", key: "e2" },
      ];
      const allData: Record<string, string> = {
        e1: "Drill",
        e2: "Hammer",
        e3: "Saw",
        e4: "Wrench",
        e5: "Pliers",
        ...(opts.materializeData ?? {}),
      };
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
              public_url: b.public_url ?? null,
              has_credential: c.has_credential || !!b.credential,
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
        has_credential: !!b.credential,
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
    if (url === "/api/templates") {
      if (opts.templates) return json({ templates: opts.templates });
      return json({ templates: [{ id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300, format: { type: "single" } }] });
    }
    if (url.startsWith("/api/templates/")) {
      const id = url.slice("/api/templates/".length);
      if (opts.templateDetails && opts.templateDetails[id]) return json(opts.templateDetails[id]);
      if (opts.templateDetail) return json(opts.templateDetail);
      if (id === "tpl") return json(templateDetail);
    }
    if (url === "/api/printers") return json(opts.printers ?? []);
    if (url.startsWith("/api/render/label") && method === "POST") {
      if (opts.renderLabel) return opts.renderLabel();
      return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
    }
    if (url === "/api/render" && method === "POST") {
      const body = init?.body ? JSON.parse(String(init.body)) : {};
      if (opts.batch) return opts.batch(body);
      return new Response(new Blob(["%PDF"]), {
        status: 200,
        headers: { "content-type": "application/pdf", "content-disposition": 'attachment; filename="tpl.pdf"' },
      });
    }
    throw new Error(`unexpected fetch: ${url} ${method}`);
  });

  return Object.assign(fn, {
    setSettings: (s: Record<string, { value: unknown; is_default: boolean }>) => { currentSettings = s; },
    setSettingsError: (err: boolean) => { settingsError = err; },
    setConnections: (conns: StubConnection[]) => { state = conns; },
    setConnectionsError: (err: boolean) => { connectionsError = err; },
  });
}

function MutationBridge() {
  const save = useSaveConnection();
  const del = useDeleteConnection();
  const setDef = useSetDefaultConnection();
  const clearDef = useClearDefaultConnection();
  return (
    <div style={{ display: "none" }}>
      <button
        data-testid="test-trigger-save"
        type="button"
        onClick={() =>
          save.mutate({
            input: {
              connector: "homebox",
              name: "New Connection",
              base_url: "http://hb-new.lan",
              credential: "secret",
            },
          })
        }
      >
        Save
      </button>
      <button
        data-testid="test-trigger-delete"
        type="button"
        onClick={() => del.mutate("c1")}
      >
        Delete
      </button>
      <button
        data-testid="test-trigger-set-default"
        type="button"
        onClick={() => setDef.mutate("c2")}
      >
        Set Default
      </button>
      <button
        data-testid="test-trigger-clear-default"
        type="button"
        onClick={() => clearDef.mutate()}
      >
        Clear Default
      </button>
      <button
        data-testid="test-trigger-update-c1"
        type="button"
        onClick={() =>
          save.mutate({
            id: "c1",
            input: {
              name: "Home 1",
              base_url: "http://hb-updated",
              credential: "secret",
            },
          })
        }
      >
        Update C1
      </button>
    </div>
  );
}

function renderConnect(client?: QueryClient, initialPath = "/connect") {
  const qc = client ?? new QueryClient({ defaultOptions: { queries: { retry: false } } });
  let currentLocation = { pathname: initialPath, state: undefined as unknown };

  function LocationTracker() {
    const loc = useLocation();
    useEffect(() => {
      currentLocation = loc;
    }, [loc]);
    return null;
  }

  const view = render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={[initialPath]}>
          <LocationTracker />
          <MutationBridge />
          <Routes>
            <Route path="/connect" element={<Connect />} />
            <Route path="/connections" element={<div data-testid="connections-page">Connections Page</div>} />
            <Route path="/connections/new" element={<div data-testid="new-connection-page">New Connection Page</div>} />
          </Routes>
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
  return { ...view, queryClient: qc, getLocation: () => currentLocation };
}

let fetchMock: ReturnType<typeof stub>;
const matches = (u: unknown, path: string) =>
  path === "/api/print" || path === "/api/render" ? String(u) === path : String(u).startsWith(path);
const countCalls = (path: string) => fetchMock.mock.calls.filter(([u]) => matches(u, path)).length;

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
    expect(within(grid).getByDisplayValue("Drill")).toBeInTheDocument();
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
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
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

  it("falls back to the first connection when no default is stored", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
      ],
      settings: {
        default_connection_id: { value: null, is_default: true },
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    const select = await screen.findByLabelText(/^connection$/i);
    await waitFor(() => expect((select as HTMLSelectElement).value).toBe("c1"));
    await waitFor(() => expect(countCalls("/api/connections/c1/browse")).toBeGreaterThan(0));
  });

  it("falls back to the first connection when the stored default names no connection", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
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

  it("falls back to the first connection when settings query errors", async () => {
    fetchMock = stub({
      connections: [
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
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
        { id: "a", connector: "homebox", name: "Home", base_url: "http://hba", has_credential: true },
        { id: "b", connector: "homebox", name: "Home", base_url: "http://hbb", has_credential: true },
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
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
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
        { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
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

  describe("8.1 Connection management link, empty-state CTA, loading, failure", () => {
    it("renders no connections table, form or default-connection control, and offers link to /connections with origin state", async () => {
      fetchMock = stub({
        connections: [
          { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", has_credential: true },
        ],
      });
      vi.stubGlobal("fetch", fetchMock);
      const { getLocation } = renderConnect();

      // Verify no connections table or form or default control
      expect(screen.queryByLabelText(/default connection/i)).not.toBeInTheDocument();
      expect(screen.queryByLabelText(/base url/i)).not.toBeInTheDocument();
      expect(screen.queryByRole("table")).not.toBeInTheDocument();

      // Offers link to /connections
      const link = await screen.findByRole("link", { name: /manage connections/i });
      expect(link).toHaveAttribute("href", "/connections");

      fireEvent.click(link);
      expect(await screen.findByTestId("connections-page")).toBeInTheDocument();
      expect(getLocation().state).toEqual({ from: "/connect" });
    });

    it("offers call to action linking to /connections/new with origin state when list loads empty", async () => {
      fetchMock = stub({ connections: [] });
      vi.stubGlobal("fetch", fetchMock);
      const { getLocation } = renderConnect();

      expect(await screen.findByText(/no connections configured/i)).toBeInTheDocument();
      const ctaLink = screen.getByRole("link", { name: /add connection/i });
      expect(ctaLink).toHaveAttribute("href", "/connections/new");

      fireEvent.click(ctaLink);
      expect(await screen.findByTestId("new-connection-page")).toBeInTheDocument();
      expect(getLocation().state).toEqual({ from: "/connect" });
    });

    it("offers no call to action while connections list has not answered", async () => {
      let release: (() => void) | undefined;
      const pendingGate = new Promise<void>((r) => { release = r; });
      fetchMock = vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/connections") {
          await pendingGate;
          return json([]);
        }
        if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stub>;
      vi.stubGlobal("fetch", fetchMock);

      renderConnect();
      expect(screen.queryByRole("link", { name: /add connection/i })).not.toBeInTheDocument();

      release?.();
      expect(await screen.findByText(/no connections configured/i)).toBeInTheDocument();
    });

    it("reports a failed list and offers no call to action", async () => {
      fetchMock = stub({ connectionsError: true });
      vi.stubGlobal("fetch", fetchMock);

      renderConnect();
      expect(await screen.findByText(/failed to load connections\./i)).toBeInTheDocument();
      expect(screen.queryByRole("link", { name: /add connection/i })).not.toBeInTheDocument();
      expect(screen.getByRole("link", { name: /manage connections/i })).toBeInTheDocument();
    });
  });

  describe("8.2 Resolution per visit", () => {
    it("returning from connection management resolves afresh without restoring hand-picked connection or its rows", async () => {
      fetchMock = stub({
        connections: [
          { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
          { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
        ],
        settings: {
          default_connection_id: { value: "c1", is_default: false },
        },
      });
      vi.stubGlobal("fetch", fetchMock);

      // Visit 1: connect opens on default c1
      const { unmount, queryClient } = renderConnect();
      const picker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));

      // Manually pick c2 and select a row
      fireEvent.change(picker, { target: { value: "c2" } });
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c2"));
      const checkbox = await screen.findByLabelText("select entities:e1");
      fireEvent.click(checkbox);
      await waitFor(() => expect(screen.getByLabelText("select entities:e1")).toBeChecked());

      // Leave Connect (unmount)
      unmount();

      // Visit 2: return to Connect
      renderConnect(queryClient);
      const newPicker = await screen.findByLabelText(/^connection$/i);
      // Resolves afresh to c1 (the stored default), NOT c2!
      await waitFor(() => expect((newPicker as HTMLSelectElement).value).toBe("c1"));
      // The hand-picked c2's row selection is gone
      const newCheckbox = await screen.findByLabelText("select entities:e1");
      expect(newCheckbox).not.toBeChecked();
    });

    it("a connection created while away resolves on return when it sorts first", async () => {
      fetchMock = stub({
        connections: [
          { id: "c2", connector: "homebox", name: "Zeta", base_url: "http://hb2", has_credential: true },
        ],
      });
      vi.stubGlobal("fetch", fetchMock);

      // Visit 1: resolves c2
      const { unmount, queryClient } = renderConnect();
      const picker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c2"));
      unmount();

      // While away: create c1 which sorts first
      const c1: StubConnection = {
        id: "c1",
        connector: "homebox",
        name: "Alpha",
        base_url: "http://hb1",
        has_credential: true,
      };
      fetchMock.setConnections([c1, { id: "c2", connector: "homebox", name: "Zeta", base_url: "http://hb2", has_credential: true }]);
      await queryClient.removeQueries({ queryKey: ["connections"] });

      // Visit 2: returns to Connect
      renderConnect(queryClient);
      const newPicker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((newPicker as HTMLSelectElement).value).toBe("c1"));
    });

    it("creating the first connection from the call to action resolves on return", async () => {
      fetchMock = stub({ connections: [] });
      vi.stubGlobal("fetch", fetchMock);

      // Visit 1: empty CTA shown
      const { unmount, queryClient } = renderConnect();
      expect(await screen.findByText(/no connections configured/i)).toBeInTheDocument();
      unmount();

      // While away: created first connection
      const c1: StubConnection = {
        id: "c1",
        connector: "homebox",
        name: "First Home",
        base_url: "http://hb1",
        has_credential: true,
      };
      fetchMock.setConnections([c1]);
      await queryClient.removeQueries({ queryKey: ["connections"] });

      // Visit 2: returns to Connect
      renderConnect(queryClient);
      const picker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));
      expect(screen.queryByText(/no connections configured/i)).not.toBeInTheDocument();
    });

    it("a connection renamed or deleted while away reflects on return", async () => {
      fetchMock = stub({
        connections: [
          { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
          { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
        ],
        settings: {
          default_connection_id: { value: "c1", is_default: false },
        },
      });
      vi.stubGlobal("fetch", fetchMock);

      const { unmount: unmount1, queryClient } = renderConnect();
      const picker1 = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker1 as HTMLSelectElement).value).toBe("c1"));
      unmount1();

      // 1. Renamed while away:
      fetchMock.setConnections([
        { id: "c1", connector: "homebox", name: "Home 1 Renamed", base_url: "http://hb1", has_credential: true },
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
      ]);
      await queryClient.removeQueries({ queryKey: ["connections"] });

      const { unmount: unmount3 } = renderConnect(queryClient);
      const picker3 = await screen.findByLabelText(/^connection$/i);
      await screen.findByRole("option", { name: "Home 1 Renamed" });
      await waitFor(() => expect((picker3 as HTMLSelectElement).value).toBe("c1"));
      unmount3();

      // 2. Deleted while away:
      fetchMock.setConnections([
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
      ]);
      fetchMock.setSettings({ default_connection_id: { value: null, is_default: true } });
      await queryClient.removeQueries({ queryKey: ["connections"] });
      await queryClient.removeQueries({ queryKey: ["connector-schema", "c1"] });
      await queryClient.removeQueries({ queryKey: ["settings"] });

      renderConnect(queryClient);
      const picker4 = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker4 as HTMLSelectElement).value).toBe("c2"));
      expect(screen.queryByRole("option", { name: /Home 1/ })).not.toBeInTheDocument();
    });
  });

  describe("8.3 Pending-write gate", () => {
    it("leaving form by Cancel during a save leaves Connect selecting nothing and reporting that it waits, then acting on post-write answers", async () => {
      let resolveSave!: (res: Response) => void;
      const savePromise = new Promise<Response>((r) => { resolveSave = r; });

      fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/connections" && method === "POST") return savePromise;
        if (url === "/api/connections" && method === "GET") {
          return json([
            { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
          ]);
        }
        if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
        if (url === "/api/templates") return json({ templates: [] });
        if (url === "/api/printers") return json([]);
        if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(schema);
        if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({ rows: [], has_more: false, count: 0 });
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stub>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      const { getLocation } = renderConnect(qc);

      // Trigger save mutation (which has mutationKey: ["connection"])
      fireEvent.click(screen.getByTestId("test-trigger-save"));

      // Connect renders waiting message, selects nothing, reads no schema
      expect(await screen.findByText(/waiting\.\.\./i)).toBeInTheDocument();
      expect((screen.getByLabelText(/^connection$/i) as HTMLSelectElement).value).toBe("");

      // Resolve save
      await act(async () => {
        resolveSave(json({ id: "c_new", connector: "homebox", name: "New Connection", base_url: "http://hb-new.lan", has_credential: true }, 201));
      });

      // Once resolved, waiting message clears and Connect resolves
      const picker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));
      expect(getLocation().pathname).toBe("/connect");
    });

    it("leaving form by primary navigation during a delete leaves Connect selecting nothing and reporting waiting, then acts on post-write answers without moving operator", async () => {
      let resolveDelete!: (res: Response) => void;
      const deletePromise = new Promise<Response>((r) => { resolveDelete = r; });
      let schemaRequested = false;

      fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/connections/c1" && method === "DELETE") return deletePromise;
        if (url === "/api/connections" && method === "GET") {
          return json([
            { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
          ]);
        }
        if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
        if (url === "/api/templates") return json({ templates: [] });
        if (url === "/api/printers") return json([]);
        if (url.includes("/api/connections/c1/schema")) {
          schemaRequested = true;
          return json(schema);
        }
        if (url.includes("/api/connections/c2/schema")) return json(schema);
        if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({ rows: [], has_more: false, count: 0 });
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stub>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      const { getLocation } = renderConnect(qc);

      // Trigger delete mutation
      fireEvent.click(screen.getByTestId("test-trigger-delete"));

      // Connect waits
      expect(await screen.findByText(/waiting\.\.\./i)).toBeInTheDocument();
      expect((screen.getByLabelText(/^connection$/i) as HTMLSelectElement).value).toBe("");

      // Resolve delete
      await act(async () => {
        resolveDelete(new Response(null, { status: 204 }));
      });

      // Post-delete: c2 resolves, c1 schema is never requested, operator stays on /connect
      const picker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c2"));
      expect(schemaRequested).toBe(false);
      expect(getLocation().pathname).toBe("/connect");
    });

    it("naming a default then opening Connect before write answers waits and resolves on newly named default", async () => {
      let resolveSetDefault!: (res: Response) => void;
      const defaultPromise = new Promise<Response>((r) => { resolveSetDefault = r; });
      let defaultSaved = false;

      fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/settings/default_connection_id" && method === "PUT") {
          defaultSaved = true;
          return defaultPromise;
        }
        if (url === "/api/connections" && method === "GET") {
          return json([
            { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
            { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
          ]);
        }
        if (url === "/api/settings" && method === "GET") {
          return json({ default_connection_id: { value: defaultSaved ? "c2" : "c1", is_default: false } });
        }
        if (url === "/api/templates") return json({ templates: [] });
        if (url === "/api/printers") return json([]);
        if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(schema);
        if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({ rows: [], has_more: false, count: 0 });
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stub>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      renderConnect(qc);

      // Trigger set default to c2
      fireEvent.click(screen.getByTestId("test-trigger-set-default"));

      // Connect waits
      expect(await screen.findByText(/waiting\.\.\./i)).toBeInTheDocument();
      expect((screen.getByLabelText(/^connection$/i) as HTMLSelectElement).value).toBe("");

      // Resolve set default
      await act(async () => {
        resolveSetDefault(json({ value: "c2", is_default: false }));
      });

      // Connect resolves on newly named default c2
      const picker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c2"));
    });

    it("clearing a default before write answers waits and clears waiting once settled", async () => {
      let resolveClearDefault!: (res: Response) => void;
      const clearPromise = new Promise<Response>((r) => { resolveClearDefault = r; });

      fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/settings/default_connection_id" && method === "DELETE") return clearPromise;
        if (url === "/api/connections" && method === "GET") {
          return json([
            { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
          ]);
        }
        if (url === "/api/settings" && method === "GET") {
          return json({ default_connection_id: { value: null, is_default: true } });
        }
        if (url === "/api/templates") return json({ templates: [] });
        if (url === "/api/printers") return json([]);
        if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(schema);
        if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({ rows: [], has_more: false, count: 0 });
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stub>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      renderConnect(qc);

      // Trigger clear default
      fireEvent.click(screen.getByTestId("test-trigger-clear-default"));

      // Connect waits
      expect(await screen.findByText(/waiting\.\.\./i)).toBeInTheDocument();
      expect((screen.getByLabelText(/^connection$/i) as HTMLSelectElement).value).toBe("");

      // Resolve clear default
      await act(async () => {
        resolveClearDefault(new Response(null, { status: 204 }));
      });

      // Connect resolves on fallback c1 once settled
      const picker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));
      expect(screen.queryByText(/waiting\.\.\./i)).not.toBeInTheDocument();
    });

    it("presents neither empty-list call to action nor failure message while reporting waiting", async () => {
      let resolveSave!: (res: Response) => void;
      const savePromise = new Promise<Response>((r) => { resolveSave = r; });

      fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/connections" && method === "POST") return savePromise;
        if (url === "/api/connections" && method === "GET") {
          return json([]);
        }
        if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
        if (url === "/api/templates") return json({ templates: [] });
        if (url === "/api/printers") return json([]);
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stub>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      renderConnect(qc);

      // Trigger save mutation (in flight)
      fireEvent.click(screen.getByTestId("test-trigger-save"));

      // Connect renders waiting message
      expect(await screen.findByText(/waiting\.\.\./i)).toBeInTheDocument();
      // While waiting, it does NOT present pre-write empty call to action or failure message
      expect(screen.queryByText(/no connections configured/i)).not.toBeInTheDocument();
      expect(screen.queryByText(/failed to load connections/i)).not.toBeInTheDocument();

      // Resolve save
      await act(async () => {
        resolveSave(json({ id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true }, 201));
      });
    });
  });

  describe("8.4 Freshness", () => {
    it("a save reaching Connect on the next visit with that connection selected browses new rows", async () => {
      let isUpdated = false;
      fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/connections/c1" && method === "PUT") {
          isUpdated = true;
          return json({ id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb-updated", has_credential: true });
        }
        if (url === "/api/connections" && method === "GET") {
          return json([
            { id: "c1", connector: "homebox", name: "Home 1", base_url: isUpdated ? "http://hb-updated" : "http://hb1", has_credential: true },
          ]);
        }
        if (url === "/api/connections/c1/schema") return json(schema);
        if (url.startsWith("/api/connections/c1/browse")) {
          return json({
            rows: [
              {
                id: { resource: "entities", key: isUpdated ? "e-new" : "e-old" },
                cells: { name: isUpdated ? "Hammer from updated upstream" : "Drill from old upstream" },
              },
            ],
            has_more: false,
            count: 1,
          });
        }
        if (url === "/api/settings") return json({ default_connection_id: { value: "c1", is_default: false } });
        if (url === "/api/templates") return json({ templates: [] });
        if (url === "/api/printers") return json([]);
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stub>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      const { unmount } = renderConnect(qc);

      // Initial visit: Connect resolves c1 and browses rows from old upstream
      expect(await screen.findByText("Drill from old upstream")).toBeInTheDocument();
      unmount();

      // Operator updates c1 to new base_url via real mutation
      const bridge = render(
        <QueryClientProvider client={qc}>
          <ToastProvider>
            <MemoryRouter>
              <MutationBridge />
            </MemoryRouter>
          </ToastProvider>
        </QueryClientProvider>,
      );
      fireEvent.click(bridge.getByTestId("test-trigger-update-c1"));
      await waitFor(() => expect(isUpdated).toBe(true));
      bridge.unmount();

      // Next visit: returning to Connect with that connection selected
      renderConnect(qc);
      const picker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));

      // The rows shown are browsed from the new upstream
      expect(await screen.findByText("Hammer from updated upstream")).toBeInTheDocument();
      expect(screen.queryByText("Drill from old upstream")).not.toBeInTheDocument();
    });

    it("no pre-write answer being presented: schema and list acted on are answers made after save", async () => {
      let saved = false;
      let connectionsFetches = 0;
      let schemaFetches = 0;

      fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/connections/c1" && method === "PUT") {
          saved = true;
          return json({ id: "c1", connector: "homebox", name: "Home Updated", base_url: "http://hb-updated", has_credential: true });
        }
        if (url === "/api/connections" && method === "GET") {
          connectionsFetches++;
          return json([
            { id: "c1", connector: "homebox", name: saved ? "Home Post-Write" : "Home Pre-Write", base_url: "http://hb", has_credential: true },
          ]);
        }
        if (url === "/api/connections/c1/schema") {
          schemaFetches++;
          return json({
            ...schema,
            resources: [
              {
                ...schema.resources[0],
                label: saved ? "Items Post-Write" : "Items Pre-Write",
              },
            ],
          });
        }
        if (url.startsWith("/api/connections/c1/browse")) {
          return json({
            rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill" } }],
            has_more: false,
            count: 1,
          });
        }
        if (url === "/api/settings") return json({ default_connection_id: { value: "c1", is_default: false } });
        if (url === "/api/templates") return json({ templates: [] });
        if (url === "/api/printers") return json([]);
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stub>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      const { unmount } = renderConnect(qc);

      // Pre-write: Connect presents pre-write answers
      expect(await screen.findByRole("option", { name: "Home Pre-Write" })).toBeInTheDocument();
      expect(await screen.findByText("Items Pre-Write")).toBeInTheDocument();
      expect(connectionsFetches).toBe(1);
      expect(schemaFetches).toBe(1);
      unmount();

      // Operator saves connection
      const bridge = render(
        <QueryClientProvider client={qc}>
          <ToastProvider>
            <MemoryRouter>
              <MutationBridge />
            </MemoryRouter>
          </ToastProvider>
        </QueryClientProvider>,
      );
      fireEvent.click(bridge.getByTestId("test-trigger-update-c1"));
      await waitFor(() => expect(saved).toBe(true));
      bridge.unmount();

      // Return to Connect: requests made after save, copies read before it are NEVER presented
      renderConnect(qc);
      expect(await screen.findByRole("option", { name: "Home Post-Write" })).toBeInTheDocument();
      expect(await screen.findByText("Items Post-Write")).toBeInTheDocument();
      expect(screen.queryByRole("option", { name: "Home Pre-Write" })).not.toBeInTheDocument();
      expect(screen.queryByText("Items Pre-Write")).not.toBeInTheDocument();
      expect(connectionsFetches).toBe(2);
      expect(schemaFetches).toBe(2);
    });

    it("a credential-only save requests schema and browses rows again on next visit", async () => {
      let schemaCalls = 0;
      let browseCalls = 0;
      fetchMock = vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/connections") {
          return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", has_credential: true }]);
        }
        if (url === "/api/settings") return json({ default_connection_id: { value: "c1", is_default: false } });
        if (url === "/api/templates") return json({ templates: [] });
        if (url === "/api/printers") return json([]);
        if (url.includes("/api/connections/c1/schema")) {
          schemaCalls++;
          return json(schema);
        }
        if (url.includes("/api/connections/c1/browse")) {
          browseCalls++;
          return json({ rows: [{ id: { resource: "entities", key: "1" }, cells: { name: "Item" } }], has_more: false, count: 1 });
        }
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stub>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      const { unmount } = renderConnect(qc);
      await screen.findByText("Item");
      expect(schemaCalls).toBe(1);
      expect(browseCalls).toBe(1);
      unmount();

      // Credential-only save evicts schema and connections
      await qc.removeQueries({ queryKey: ["connections"] });
      await qc.removeQueries({ queryKey: ["connector-schema", "c1"] });

      renderConnect(qc);
      await screen.findByText("Item");
      expect(schemaCalls).toBe(2);
      expect(browseCalls).toBe(2);
    });

    it("failed write changes nothing and Connect proceeds on cached answers", async () => {
      let failSave = false;
      fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/connections" && method === "POST") {
          if (failSave) return json({ error: "Server error" }, 500);
          return json({ id: "c_new" }, 201);
        }
        if (url === "/api/connections" && method === "GET") {
          return json([
            { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
          ]);
        }
        if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
        if (url === "/api/templates") return json({ templates: [] });
        if (url === "/api/printers") return json([]);
        if (url.startsWith("/api/connections/c1/schema")) return json(schema);
        if (url.startsWith("/api/connections/c1/browse")) {
          return json({ rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill" } }], has_more: false, count: 1 });
        }
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stub>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
      renderConnect(qc);
      await screen.findByText("Drill");

      // Verify queries are cached
      expect(qc.getQueryData(["connections"])).toBeDefined();
      expect(qc.getQueryData(["connector-schema", "c1"])).toBeDefined();

      // Configure save to fail with 500
      failSave = true;

      // Trigger real save mutation and wait for it to fail
      fireEvent.click(screen.getByTestId("test-trigger-save"));
      await waitFor(() => {
        const failedCalls = fetchMock.mock.calls.filter(([u, init]) => String(u) === "/api/connections" && ((init as RequestInit)?.method ?? "GET").toUpperCase() === "POST");
        expect(failedCalls.length).toBe(1);
      });

      // Wait until mutation has settled
      await waitFor(() => expect(screen.queryByText(/waiting\.\.\./i)).not.toBeInTheDocument());

      // Failed write did NOT evict anything: cache is intact
      expect(qc.getQueryData(["connections"])).toBeDefined();
      expect(qc.getQueryData(["connector-schema", "c1"])).toBeDefined();

      // Connect proceeds on cached answers; rows ("Drill") remain on screen
      expect(screen.getByText("Drill")).toBeInTheDocument();
    });

    it("default write does not evict connections list", async () => {
      let connectionsFetches = 0;
      let settingsFetches = 0;
      fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/connections" && method === "GET") {
          connectionsFetches++;
          return json([
            { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
            { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
          ]);
        }
        if (url === "/api/settings/default_connection_id" && method === "PUT") {
          return json({ value: "c2", is_default: false });
        }
        if (url === "/api/settings" && method === "GET") {
          settingsFetches++;
          return json({ default_connection_id: { value: "c1", is_default: false } });
        }
        if (url === "/api/templates") return json({ templates: [] });
        if (url === "/api/printers") return json([]);
        if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(schema);
        if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({ rows: [], has_more: false, count: 0 });
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stub>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 60_000 } } });
      renderConnect(qc);
      const picker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));
      expect(connectionsFetches).toBe(1);
      expect(settingsFetches).toBe(1);

      // Trigger the real useSetDefaultConnection mutation via test bridge
      fireEvent.click(screen.getByTestId("test-trigger-set-default"));

      // Wait for mutation to complete
      await waitFor(() => {
        const putCalls = fetchMock.mock.calls.filter(([u, i]) => String(u) === "/api/settings/default_connection_id" && ((i as RequestInit)?.method ?? "GET").toUpperCase() === "PUT");
        expect(putCalls.length).toBe(1);
      });
      await waitFor(() => expect(screen.queryByText(/waiting\.\.\./i)).not.toBeInTheDocument());

      // Settings was evicted and refetched by active observer
      await waitFor(() => expect(settingsFetches).toBe(2));

      // Connections query was NOT evicted by the mutation
      expect(qc.getQueryData(["connections"])).toBeDefined();

      // connections was not re-fetched
      expect(connectionsFetches).toBe(1);
    });

    it("deleted connection leaves no held schema and draws no request", async () => {
      let schemaC1Calls = 0;
      let deleteResolved = false;
      fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/connections/c1" && method === "DELETE") {
          deleteResolved = true;
          return new Response(null, { status: 204 });
        }
        if (url === "/api/connections" && method === "GET") {
          return json(
            deleteResolved
              ? [{ id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true }]
              : [
                  { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
                  { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
                ],
          );
        }
        if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
        if (url === "/api/templates") return json({ templates: [] });
        if (url === "/api/printers") return json([]);
        if (url.includes("/api/connections/c1/schema")) {
          schemaC1Calls++;
          return json(schema);
        }
        if (url.includes("/api/connections/c2/schema")) return json(schema);
        if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({ rows: [], has_more: false, count: 0 });
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stub>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      const { unmount } = renderConnect(qc);

      // Initial visit: Connect resolves on c1 and loads c1 schema
      const picker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));
      expect(schemaC1Calls).toBe(1);
      expect(qc.getQueryData(["connector-schema", "c1"])).toBeDefined();

      // Operator triggers delete of c1 via real useDeleteConnection hook
      fireEvent.click(screen.getByTestId("test-trigger-delete"));
      await waitFor(() => expect(deleteResolved).toBe(true));

      // After delete mutation settles, c1 schema is no longer held in query client
      expect(qc.getQueryData(["connector-schema", "c1"])).toBeUndefined();

      unmount();

      // Next visit to Connect: c1 is deleted, resolves on c2, and draws NO schema request for c1
      renderConnect(qc);
      const picker2 = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker2 as HTMLSelectElement).value).toBe("c2"));
      expect(schemaC1Calls).toBe(1); // No new request for c1 schema
      expect(qc.getQueryData(["connector-schema", "c1"])).toBeUndefined();
    });
  });

  describe("8.5 Surviving selection rules", () => {
    it("rows selected against a connection do not come back when connection is cleared and another is picked", async () => {
      fetchMock = stub({
        connections: [
          { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
          { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
        ],
      });
      vi.stubGlobal("fetch", fetchMock);

      const { queryClient } = renderConnect();
      const picker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));

      // Select two rows on c1
      fireEvent.click(await screen.findByLabelText("select entities:e1"));
      fireEvent.click(await screen.findByLabelText("select entities:e2"));
      expect(screen.getByLabelText("select entities:e1")).toBeChecked();
      expect(screen.getByLabelText("select entities:e2")).toBeChecked();

      // Delete c1 in background (e.g. another operator deleted it)
      fetchMock.setConnections([
        { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
      ]);
      await queryClient.invalidateQueries({ queryKey: ["connections"] });

      // Picker clears to ""
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
      expect(within(grid).getByDisplayValue("Drill")).toBeInTheDocument();
      expect(within(grid).queryByText("Hammer")).not.toBeInTheDocument();
    });

    it("leaves selected connection, browse table and row selection unchanged when a later connections request fails", async () => {
      fetchMock = stub({
        connections: [
          { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
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

    it("naming a different connection as default while working on one leaves selection unchanged", async () => {
      fetchMock = stub({
        connections: [
          { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", has_credential: true },
          { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", has_credential: true },
        ],
        settings: {
          default_connection_id: { value: "c1", is_default: false },
        },
      });
      vi.stubGlobal("fetch", fetchMock);

      const { queryClient } = renderConnect();
      const picker = await screen.findByLabelText(/^connection$/i);
      await waitFor(() => expect((picker as HTMLSelectElement).value).toBe("c1"));

      // Select row
      const checkbox = await screen.findByLabelText("select entities:e1");
      fireEvent.click(checkbox);
      await waitFor(() => expect(screen.getByLabelText("select entities:e1")).toBeChecked());

      // Setting changes in background to c2
      fetchMock.setSettings({ default_connection_id: { value: "c2", is_default: false } });
      await queryClient.invalidateQueries({ queryKey: ["settings"] });

      // Selected connection in main picker and row selection are unchanged
      expect((picker as HTMLSelectElement).value).toBe("c1");
      expect(screen.getByLabelText("select entities:e1")).toBeChecked();
    });
  });
});

// #209: the connector grid applies the same datetime cell rule as the CSV grid. A materialized row
// leaves the parameter blank, which is valid; an edited cell that cannot be parsed blocks the run.
describe("Connect: datetime parameters", () => {
  const dtDetail = {
    ...templateDetail,
    params: [
      { name: "name", type: "string" as const, control: "text" as const },
      { name: "printed_on", type: "datetime" as const, control: "datetime" as const, description: "Print date" },
    ],
  };

  // The datetime template, plus a connector that offers a `printed_on` field so the default mapping
  // carries `value` into every materialized row.
  const withPrintedOn = (value?: string) => {
    const base = stub();
    return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const tDetail = {
        ...dtDetail,
        params: [
          { name: "name", type: "string" as const, control: "text" as const },
          { name: "printed_on", type: "datetime" as const, control: "datetime" as const, time: true, description: "Print date" },
        ],
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
      if (url === "/api/connections") return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", has_credential: true }]);
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(multiValuedSchema);
      if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({ rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill", tags: ["KIDS"] } }], has_more: false, count: 1 });
      if (url.startsWith("/api/connections/") && url.endsWith("/materialize")) return json([{ source: { resource: "entities", key: "e1" }, data: { name: "Drill", tags: ["KIDS"] } }]);
      if (url === "/api/templates") return json({ templates: [{ id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300, format: { type: "single" } }] });
      if (url === "/api/templates/tpl") {
        return json({
          ...templateDetail,
          params: [
            { name: "title", type: "string", control: "text" },
            { name: "tagList", type: "list", control: "list" },
          ],
        });
      }
      if (url === "/api/printers") return json([]);
      if (url.startsWith("/api/render/label") && method === "POST") return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url === "/api/render" && method === "POST") return new Response(new Blob(["%PDF"]), { status: 200, headers: { "content-type": "application/pdf" } });
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
    expect(within(grid).getByDisplayValue("Drill")).toBeInTheDocument();
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
      if (url === "/api/connections") return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", has_credential: true }]);
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(multiValuedSchema);
      if (url.startsWith("/api/connections/") && url.endsWith("/browse")) return json({
        rows: [
          { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", tags: ["KIDS", "CONSUMABLE"] } },
          { id: { resource: "entities", key: "e2" }, cells: { name: "Hammer", tags: [] } },
        ],
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
          params: [
            { name: "name", type: "string", control: "text" },
            { name: "tags", type: "list", control: "list" },
          ],
        });
      }
      if (url === "/api/printers") return json([]);
      if (url.startsWith("/api/render/label") && method === "POST") return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      if (url === "/api/render" && method === "POST") {
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
});

describe("issue-386: sheet preview", () => {
  const sheetTemplateDetail = {
    id: "sheet-tpl",
    name: "Sheet",
    description: "",
    unit: "mm",
    dpi: 300,
    format: {
      type: "sheet",
      width: 210,
      height: 297,
      positions: Array.from({ length: 30 }, () => ({ x: 0, y: 0 })),
    },
    params: [{ name: "name", type: "string", control: "text" }],
  };

  type BatchPayload = {
    mode?: string; // asserted absent: /api/render has no mode
    template?: string;
    start_slot?: number;
    labels?: Array<{ data: Record<string, unknown> }>;
  };

  async function selectSheetAndMaterialize(opts?: StubOptions, selectCount = 2) {
    fetchMock = stub({
      templates: [
        { id: "sheet-tpl", name: "Sheet", description: "", unit: "mm", dpi: 300, format: { type: "sheet" } },
        { id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300, format: { type: "single" } },
      ],
      templateDetails: {
        "sheet-tpl": sheetTemplateDetail,
        tpl: templateDetail,
      },
      ...opts,
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await screen.findByRole("option", { name: "Home" });
    fireEvent.change(await screen.findByLabelText(/^connection$/i), { target: { value: "c1" } });
    fireEvent.change(await screen.findByLabelText(/template/i), { target: { value: "sheet-tpl" } });
    for (let i = 1; i <= selectCount; i++) {
      fireEvent.click(await screen.findByLabelText(`select entities:e${i}`));
    }
    fireEvent.click(await screen.findByRole("button", { name: /add .* row/i }));
    await screen.findByRole("grid", { name: /label rows/i });
  }

  it("5.1 Sheet template with 2 valid rows, copies 3 and start slot 1: one POST /api/render preview request with 6 labels in row order and start_slot: 1; activating Download then sends a body whose labels and start_slot deep-equal the preview's", async () => {
    let capturedBatchBodies: BatchPayload[] = [];
    await selectSheetAndMaterialize({
      batch: (body) => {
        capturedBatchBodies.push(body);
        return new Response(new Blob(["%PDF"]), {
          status: 200,
          headers: { "content-type": "application/pdf" },
        });
      },
    });

    await waitFor(() => expect(capturedBatchBodies.length).toBe(1));
    capturedBatchBodies = [];

    fireEvent.change(screen.getByLabelText(/copies/i), { target: { value: "3" } });
    fireEvent.change(screen.getByLabelText(/start slot/i), { target: { value: "1" } });

    await waitFor(() => expect(capturedBatchBodies.length).toBe(1));
    const previewBody = capturedBatchBodies[0];
    expect(previewBody.mode).toBeUndefined();
    expect(previewBody.template).toBe("sheet-tpl");
    expect(previewBody.start_slot).toBe(1);
    expect(previewBody.labels).toEqual([
      { data: { name: "Drill" } },
      { data: { name: "Drill" } },
      { data: { name: "Drill" } },
      { data: { name: "Hammer" } },
      { data: { name: "Hammer" } },
      { data: { name: "Hammer" } },
    ]);
    expect(screen.getByLabelText(/Sheet preview/i).tagName).toBe("OBJECT");

    capturedBatchBodies = [];
    const downloadBtn = screen.getByRole("button", { name: /^download$/i });
    fireEvent.click(downloadBtn);

    await waitFor(() => expect(capturedBatchBodies.length).toBe(1));
    const downloadBody = capturedBatchBodies[0];
    expect(downloadBody.labels).toEqual(previewBody.labels);
    expect(downloadBody.start_slot).toEqual(previewBody.start_slot);
  });

  it("5.2 Sheet template renders no input[name=\"preview-row\"]; a single template renders one radio per row and the existing selected-row preview tests stay green", async () => {
    fetchMock = stub({
      templates: [
        { id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300, format: { type: "single" } },
        { id: "sheet-tpl", name: "Sheet", description: "", unit: "mm", dpi: 300, format: { type: "sheet" } },
      ],
      templateDetails: {
        tpl: templateDetail,
        "sheet-tpl": sheetTemplateDetail,
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await browseSelectMaterialize();
    expect(document.querySelectorAll('input[name="preview-row"]').length).toBe(2);

    fireEvent.change(screen.getByLabelText(/template/i), { target: { value: "sheet-tpl" } });
    await waitFor(() => {
      expect(screen.queryByRole("grid", { name: /label rows/i })).toBeNull();
    });
    fireEvent.click(await screen.findByRole("button", { name: /add 2 rows/i }));
    await screen.findByRole("grid", { name: /label rows/i });
    expect(document.querySelectorAll('input[name="preview-row"]').length).toBe(0);
  });

  // A row the grid refuses is one holding a datetime it cannot parse. It arrives the way connector
  // values do, through a `printed_on` column the default mapping carries onto the parameter.
  async function selectDatedSheet(printedOn: string[], opts?: StubOptions) {
    const base = stub({
      templates: [{ id: "sheet-tpl", name: "Sheet", description: "", unit: "mm", dpi: 300, format: { type: "sheet" } }],
      templateDetails: {
        "sheet-tpl": {
          ...sheetTemplateDetail,
          params: [
            { name: "name", type: "string", control: "text" },
            { name: "printed_on", type: "datetime", control: "datetime", time: true },
          ],
        },
      },
      browseRows: printedOn.map((_, i) => ({ id: { resource: "entities", key: `e${i + 1}` }, cells: { name: `item ${i + 1}` } })),
      ...opts,
    });
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === "/api/connections/c1/schema") {
        const printedCol = { key: "printed_on", label: "Printed", ty: "text", tier: "cheap", multi_valued: false };
        return json({ ...schema, resources: [{ ...schema.resources[0], columns: [...schema.resources[0].columns, printedCol] }] });
      }
      if (url === "/api/connections/c1/materialize") {
        return json(printedOn.map((value, i) => ({
          source: { resource: "entities", key: `e${i + 1}` },
          data: { name: `item ${i + 1}`, printed_on: value },
        })));
      }
      return base(input, init);
    }) as ReturnType<typeof stub>;
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await screen.findByRole("option", { name: "Home" });
    fireEvent.change(await screen.findByLabelText(/^connection$/i), { target: { value: "c1" } });
    fireEvent.change(await screen.findByLabelText(/template/i), { target: { value: "sheet-tpl" } });
    for (let i = 1; i <= printedOn.length; i++) {
      fireEvent.click(await screen.findByLabelText(`select entities:e${i}`));
    }
    fireEvent.click(await screen.findByRole("button", { name: /add .* row/i }));
    await screen.findByRole("grid", { name: /label rows/i });
  }

  it("5.3 Sheet template with one row holding an unparseable datetime: no /api/render request, no <object>, pane reads Fix row N to preview the sheet.; fixing the cell sends one batch request holding every row and the pane embeds the PDF", async () => {
    const capturedBatchBodies: BatchPayload[] = [];
    await selectDatedSheet(["", "not a date"], {
      batch: (body) => {
        capturedBatchBodies.push(body);
        return new Response(new Blob(["%PDF"]), {
          status: 200,
          headers: { "content-type": "application/pdf" },
        });
      },
    });

    await waitFor(() => {
      expect(screen.getByText("Fix row 2 to preview the sheet.")).toBeInTheDocument();
    });
    await new Promise((r) => setTimeout(r, 400));
    expect(document.querySelector("object")).toBeNull();
    expect(capturedBatchBodies.length).toBe(0);

    const grid = screen.getByRole("grid", { name: /label rows/i });
    const pickers = within(grid).getAllByLabelText("edit printed_on");
    fireEvent.change(pickers[1], { target: { value: "2026-08-19T10:00" } });
    await waitFor(() => {
      expect(capturedBatchBodies.length).toBe(1);
    });
    expect(capturedBatchBodies[0].labels).toEqual([
      { data: { name: "item 1" } },
      { data: { name: "item 2", printed_on: "2026-08-19T10:00" } },
    ]);
    await waitFor(() => {
      expect(document.querySelector("object")).not.toBeNull();
    });
  });

  it("5.4 Sheet template with a 5-row grid whose rows 2 and 5 are invalid: pane reads exactly Fix rows 2, 5 to preview the sheet. and the text does not begin with Preview failed", async () => {
    await selectDatedSheet(["", "nope", "", "", "2026-02-30"]);

    await waitFor(() => {
      expect(screen.getByText("Fix rows 2, 5 to preview the sheet.")).toBeInTheDocument();
    });
    expect(screen.queryByText(/Preview failed/)).toBeNull();
  });

  it("5.5 Sheet template with 2 rows and copies set to 300: no /api/render request and the pane reads Over the 500-label limit; reduce the batch to preview the sheet.", async () => {
    let capturedBatchBodies: BatchPayload[] = [];
    await selectSheetAndMaterialize({
      batch: (body) => {
        capturedBatchBodies.push(body);
        return new Response(new Blob(["%PDF"]), {
          status: 200,
          headers: { "content-type": "application/pdf" },
        });
      },
    });

    await waitFor(() => expect(capturedBatchBodies.length).toBeGreaterThan(0));
    capturedBatchBodies = [];

    fireEvent.change(screen.getByLabelText(/copies/i), { target: { value: "300" } });

    await waitFor(() => {
      expect(
        screen.getByText("Over the 500-label limit; reduce the batch to preview the sheet."),
      ).toBeInTheDocument();
    });
    await new Promise((r) => setTimeout(r, 400));
    expect(capturedBatchBodies.length).toBe(0);
  });

  it("5.7 Single template: select row 2, clear its value, and stub /api/render/label to return a non-2xx envelope; row 2 stays the row requested, the pane shows Preview failed: with the service's message, and Download stays enabled", async () => {
    let shouldFailRender = false;

    fetchMock = stub({
      templates: [
        { id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300, format: { type: "single" } },
      ],
      templateDetails: {
        tpl: {
          ...templateDetail,
          params: [{ name: "name", type: "string", control: "text" }],
        },
      },
      renderLabel: () => {
        if (shouldFailRender) {
          return new Response(JSON.stringify({ error: { message: "cannot render empty label" } }), {
            status: 422,
            headers: { "content-type": "application/json" },
          });
        }
        return new Response(new Blob(["img"]), { status: 200, headers: { "content-type": "image/png" } });
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    renderConnect();
    await browseSelectMaterialize();

    fireEvent.click(screen.getByLabelText("preview row 2"));

    shouldFailRender = true;
    const grid = screen.getByRole("grid", { name: /label rows/i });
    const textboxes = within(grid).getAllByRole("textbox", { name: /edit name/i });
    fireEvent.change(textboxes[1], { target: { value: "" } });

    await waitFor(() => {
      expect(screen.getByText("Preview failed: cannot render empty label")).toBeInTheDocument();
    });

    const radio2 = screen.getByLabelText("preview row 2") as HTMLInputElement;
    expect(radio2.checked).toBe(true);
    expect(screen.getByRole("button", { name: /^download$/i })).toBeEnabled();
  });

  it("5.8 Sheet template: a settled edit to a cell, then to copies, then to start slot, each sends one batch request carrying the new labels count or start_slot; a re-render with the batch unchanged sends none", async () => {
    const batchCalls: BatchPayload[] = [];
    await selectSheetAndMaterialize({
      printers: [{ id: "p1", name: "Printer 1" }],
      batch: (body) => {
        batchCalls.push(body);
        return new Response(new Blob(["%PDF"]), {
          status: 200,
          headers: { "content-type": "application/pdf" },
        });
      },
    });

    await waitFor(() => expect(batchCalls.length).toBe(1));
    expect(batchCalls[0]?.labels?.[0]?.data?.name).toBe("Drill");

    // 1. Settled edit to a cell
    const grid = screen.getByRole("grid", { name: /label rows/i });
    const textboxes = within(grid).getAllByRole("textbox", { name: /edit name/i });
    fireEvent.change(textboxes[0], { target: { value: "Cordless Drill" } });

    await waitFor(() => expect(batchCalls.length).toBe(2));
    expect(batchCalls[1]?.labels?.[0]?.data?.name).toBe("Cordless Drill");

    // 2. Edit copies
    fireEvent.change(screen.getByLabelText(/copies/i), { target: { value: "2" } });
    await waitFor(() => expect(batchCalls.length).toBe(3));
    expect(batchCalls[2]?.labels?.length).toBe(4);

    // 3. Edit start slot
    fireEvent.change(screen.getByLabelText(/start slot/i), { target: { value: "3" } });
    await waitFor(() => expect(batchCalls.length).toBe(4));
    expect(batchCalls[3].start_slot).toBe(3);

    // 4. Re-render with batch unchanged
    fireEvent.change(screen.getByLabelText(/^printer$/i), { target: { value: "p1" } });
    await new Promise((r) => setTimeout(r, 400));
    expect(batchCalls.length).toBe(4);
  });
});

// #413: every screen works from the template detail's published `params`; the per-label inputs
// endpoint is gone, and a checkbox always sends its value.
describe("issue-413: Connect reads the published parameter list", () => {
  const paramsDetail = {
    id: "tpl", name: "Tape", description: "", categories: [], unit: "mm", dpi: 300,
    format: { type: "single" },
    params: [
      { name: "name", type: "string", control: "text" },
      { name: "code", type: "string", control: "text" },
      { name: "flag", type: "boolean", control: "checkbox" },
      { name: "on", type: "boolean", control: "checkbox", default: true },
    ],
    variables: [],
  };
  const inputsCalls = () => fetchMock.mock.calls.filter(([u]) => String(u).includes("/inputs"));

  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:preview");
    vi.spyOn(URL, "revokeObjectURL").mockReturnValue(undefined);
    fetchMock = stub({ templateDetail: paramsDetail });
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

  it("A18: offers mappings and columns from detail.params and never requests /inputs", async () => {
    renderConnect();
    await browseSelectMaterialize();
    for (const p of paramsDetail.params) {
      expect(screen.getByLabelText(`map ${p.name}`)).toBeInTheDocument();
    }
    const grid = screen.getByRole("grid", { name: /label rows/i });
    const headers = within(grid).getAllByRole("columnheader").map((h) => h.textContent);
    expect(headers.filter((h) => paramsDetail.params.some((p) => p.name === h))).toEqual(["name", "code", "flag", "on"]);
    expect(inputsCalls()).toEqual([]);
  });

  it("A20: an untouched row with blank checkbox cells submits each start state", async () => {
    renderConnect();
    await browseSelectMaterialize();
    await waitFor(() => expect(countCalls("/api/render/label")).toBeGreaterThan(0));
    const previewCall = [...fetchMock.mock.calls].reverse().find(([u]) => String(u).startsWith("/api/render/label"))!;
    expect(JSON.parse(String((previewCall[1] as RequestInit).body)).data).toEqual({ name: "Drill", code: "", flag: false, on: true });

    const download = screen.getByRole("button", { name: /^download$/i });
    await waitFor(() => expect(download).toBeEnabled());
    fireEvent.click(download);
    await waitFor(() => expect(countCalls("/api/render")).toBe(1));
    const batchCall = fetchMock.mock.calls.find(([u]) => String(u) === "/api/render")!;
    expect(JSON.parse(String((batchCall[1] as RequestInit).body)).labels).toEqual([
      { data: { name: "Drill", code: "", flag: false, on: true } },
      { data: { name: "Hammer", code: "", flag: false, on: true } },
    ]);
  });
});
