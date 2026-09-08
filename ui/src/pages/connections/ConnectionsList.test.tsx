import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Routes, Route, useLocation } from "react-router-dom";
import { ToastProvider } from "../../app/toast";
import { ConnectionsList } from "./ConnectionsList";
import { ConnectionForm } from "./ConnectionForm";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

type C = {
  id: string;
  connector: string;
  name: string;
  base_url: string;
  public_url?: string | null;
  enabled: boolean;
  has_credential: boolean;
  transforms: [];
};

function stubFetch(
  initialConnections: C[] = [],
  initialSettings: Record<string, { value: unknown; is_default: boolean }> = {
    default_connection_id: { value: null, is_default: true },
  },
) {
  let state: C[] = [...initialConnections];
  let settingsState = { ...initialSettings };
  return vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (input, init) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.startsWith("/api/connections/") && method === "DELETE") {
      const id = decodeURIComponent(url.slice("/api/connections/".length));
      state = state.filter((c) => c.id !== id);
      if (settingsState.default_connection_id?.value === id) {
        settingsState = {
          ...settingsState,
          default_connection_id: { value: null, is_default: true },
        };
      }
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/connections/") && url.endsWith("/schema") && method === "GET") {
      return json({ version: "1.0", resources: [], relationships: [] });
    }
    if (url === "/api/connections" && method === "GET") return json(state);
    if (url === "/api/settings" && method === "GET") return json(settingsState);
    if (url === "/api/settings/default_connection_id" && method === "PUT") {
      const b = JSON.parse(init!.body as string);
      settingsState = {
        ...settingsState,
        default_connection_id: { value: b.value, is_default: false },
      };
      return json({ value: b.value, is_default: false });
    }
    if (url === "/api/settings/default_connection_id" && method === "DELETE") {
      settingsState = {
        ...settingsState,
        default_connection_id: { value: null, is_default: true },
      };
      return new Response(null, { status: 204 });
    }
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderConnectionsList({
  client,
  initialEntries = ["/connections"],
}: {
  client?: QueryClient;
  initialEntries?: Array<string | { pathname: string; state?: unknown }>;
} = {}) {
  const qc = client ?? new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={initialEntries}>
          <ConnectionsList />
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("ConnectionsList", () => {
  let fetchMock: ReturnType<typeof stubFetch>;

  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("7.2 renders table columns, '-' for missing public url, Edit links without delete, and Add connection link", async () => {
    fetchMock = stubFetch([
      {
        id: "c1",
        connector: "homebox",
        name: "Homebox 1",
        base_url: "http://hb1.lan",
        public_url: null,
        enabled: true,
        has_credential: true,
        transforms: [],
      },
      {
        id: "c2",
        connector: "homebox",
        name: "Homebox 2",
        base_url: "http://hb2.lan",
        public_url: "https://hb2.example.com",
        enabled: false,
        has_credential: false,
        transforms: [],
      },
    ]);
    vi.stubGlobal("fetch", fetchMock);

    renderConnectionsList();

    expect(await screen.findByText("Homebox 1")).toBeInTheDocument();
    expect(screen.getByText("Homebox 2")).toBeInTheDocument();

    // Headers
    expect(screen.getByText("Name")).toBeInTheDocument();
    expect(screen.getByText("Connector")).toBeInTheDocument();
    expect(screen.getByText("Base URL")).toBeInTheDocument();
    expect(screen.getByText("Public URL")).toBeInTheDocument();
    expect(screen.getByText("API key")).toBeInTheDocument();
    expect(screen.getByText("Enabled")).toBeInTheDocument();

    // Values
    expect(screen.getByText("http://hb1.lan")).toBeInTheDocument();
    expect(screen.getByText("-")).toBeInTheDocument();
    expect(screen.getByText("set")).toBeInTheDocument();
    expect(screen.getByText("yes")).toBeInTheDocument();

    expect(screen.getByText("http://hb2.lan")).toBeInTheDocument();
    expect(screen.getByText("https://hb2.example.com")).toBeInTheDocument();
    expect(screen.getByText("none")).toBeInTheDocument();
    expect(screen.getByText("no")).toBeInTheDocument();

    // Edit link and no delete button in the row
    const editLinks = screen.getAllByRole("link", { name: "Edit" });
    expect(editLinks).toHaveLength(2);
    expect(editLinks[0]).toHaveAttribute("href", "/connections/c1");
    expect(editLinks[1]).toHaveAttribute("href", "/connections/c2");
    expect(screen.queryByRole("button", { name: "Delete" })).not.toBeInTheDocument();

    // Add connection link
    const addLink = screen.getByRole("link", { name: "Add connection" });
    expect(addLink).toHaveAttribute("href", "/connections/new");
  });

  it("7.2 distinguishes loading, failed, and loaded-empty states", async () => {
    // 1. Loading
    const pendingPromise = new Promise<Response>(() => {});
    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/connections") return pendingPromise;
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      throw new Error(`unexpected fetch: ${url}`);
    }));

    const { unmount } = renderConnectionsList();
    expect(screen.getByText("Loading connections...")).toBeInTheDocument();
    unmount();

    // 2. Failed
    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/connections") return json({ error: "failed" }, 500);
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      throw new Error(`unexpected fetch: ${url}`);
    }));

    const { unmount: unmountFailed } = renderConnectionsList();
    expect(await screen.findByText("Failed to load connections.")).toBeInTheDocument();
    expect(screen.queryByText("No connections configured.")).not.toBeInTheDocument();
    unmountFailed();

    // 3. Loaded empty
    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/connections") return json([]);
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      throw new Error(`unexpected fetch: ${url}`);
    }));

    renderConnectionsList();
    expect(await screen.findByText("No connections configured.")).toBeInTheDocument();
    expect(screen.queryByText("Failed to load connections.")).not.toBeInTheDocument();
  });

  it("7.3 default connection control: choosing, clearing, no default stored", async () => {
    fetchMock = stubFetch([
      { id: "c1", connector: "homebox", name: "Main", base_url: "http://hb", enabled: true, has_credential: true, transforms: [] },
      { id: "c2", connector: "homebox", name: "Secondary", base_url: "http://hb2", enabled: true, has_credential: true, transforms: [] },
    ]);
    vi.stubGlobal("fetch", fetchMock);

    renderConnectionsList();

    expect(await screen.findByText("Main")).toBeInTheDocument();

    const select = (await screen.findByLabelText("default connection")) as HTMLSelectElement;
    expect(select.value).toBe("");

    // Choose c1
    fireEvent.change(select, { target: { value: "c1" } });
    await waitFor(() => {
      const calls = fetchMock.mock.calls.filter(([u, i]) => String(u) === "/api/settings/default_connection_id" && ((i as RequestInit)?.method ?? "GET").toUpperCase() === "PUT");
      expect(calls.length).toBe(1);
      expect(JSON.parse(calls[0][1]!.body as string)).toEqual({ value: "c1" });
    });
    await waitFor(() => expect((screen.getByLabelText("default connection") as HTMLSelectElement).value).toBe("c1"));

    // Clear default
    fireEvent.change(select, { target: { value: "" } });
    await waitFor(() => {
      const delCalls = fetchMock.mock.calls.filter(([u, i]) => String(u) === "/api/settings/default_connection_id" && ((i as RequestInit)?.method ?? "GET").toUpperCase() === "DELETE");
      expect(delCalls.length).toBe(1);
    });
    await waitFor(() => expect((screen.getByLabelText("default connection") as HTMLSelectElement).value).toBe(""));
  });

  it("7.3 distinguishes identically named connections by id and marks disabled connection", async () => {
    fetchMock = stubFetch([
      { id: "c1", connector: "homebox", name: "Homebox", base_url: "http://hb1", enabled: true, has_credential: true, transforms: [] },
      { id: "c2", connector: "homebox", name: "Homebox", base_url: "http://hb2", enabled: false, has_credential: true, transforms: [] },
    ]);
    vi.stubGlobal("fetch", fetchMock);

    renderConnectionsList();

    expect((await screen.findAllByText("Homebox")).length).toBeGreaterThan(0);

    const select = await screen.findByLabelText("default connection");
    expect(select).toContainElement(screen.getByRole("option", { name: "Homebox (c1)" }));
    expect(select).toContainElement(screen.getByRole("option", { name: "Homebox (c2) (disabled)" }));
  });

  it("7.3 shows unavailable state for stored default no connection has, only once list has answered", async () => {
    const pendingConnections = new Promise<Response>(() => {});

    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/connections") return pendingConnections;
      if (url === "/api/settings") return json({ default_connection_id: { value: "dangling-id", is_default: false } });
      throw new Error(`unexpected fetch: ${url}`);
    }));

    const { unmount } = renderConnectionsList();

    // While list is loading, do NOT report as unavailable
    const selectLoading = (await screen.findByLabelText("default connection")) as HTMLSelectElement;
    expect(selectLoading).toBeDisabled();
    expect(screen.queryByText(/unavailable/i)).not.toBeInTheDocument();

    unmount();

    // Now with loaded list that does not hold dangling-id
    const fetchLoaded = stubFetch(
      [{ id: "c1", connector: "homebox", name: "Other", base_url: "http://hb", enabled: true, has_credential: true, transforms: [] }],
      { default_connection_id: { value: "dangling-id", is_default: false } },
    );
    vi.stubGlobal("fetch", fetchLoaded);

    renderConnectionsList();

    expect(await screen.findByText("Other")).toBeInTheDocument();

    const select = (await screen.findByLabelText("default connection")) as HTMLSelectElement;
    expect(screen.getByRole("option", { name: "dangling-id (unavailable)" })).toBeInTheDocument();
    expect(select.value).toBe("dangling-id");

    // Can still clear it
    fireEvent.change(select, { target: { value: "" } });
    await waitFor(() => {
      const calls = fetchLoaded.mock.calls.filter(([u, i]) => String(u) === "/api/settings/default_connection_id" && ((i as RequestInit)?.method ?? "GET").toUpperCase() === "DELETE");
      expect(calls.length).toBe(1);
    });
  });

  it("7.3 does not report stored default as unavailable when connections list failed", async () => {
    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/connections") return json({ error: "failed" }, 500);
      if (url === "/api/settings") return json({ default_connection_id: { value: "dangling-id", is_default: false } });
      throw new Error(`unexpected fetch: ${url}`);
    }));

    renderConnectionsList();

    expect(await screen.findByText("Failed to load connections.")).toBeInTheDocument();

    const select = (await screen.findByLabelText("default connection")) as HTMLSelectElement;
    expect(select).toBeDisabled();
    expect(screen.queryByText(/unavailable/i)).not.toBeInTheDocument();
  });

  it("7.3 shows no default after deleting the connection that was the default from its form", async () => {
    fetchMock = stubFetch(
      [
        { id: "c1", connector: "homebox", name: "Homebox 1", base_url: "http://hb1", enabled: true, has_credential: true, transforms: [] },
        { id: "c2", connector: "homebox", name: "Homebox 2", base_url: "http://hb2", enabled: true, has_credential: true, transforms: [] },
      ],
      { default_connection_id: { value: "c1", is_default: false } },
    );
    vi.stubGlobal("fetch", fetchMock);

    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
        <ToastProvider>
          <MemoryRouter initialEntries={["/connections/c1"]}>
            <Routes>
              <Route path="/connections" element={<ConnectionsList />} />
              <Route path="/connections/:id" element={<ConnectionForm />} />
            </Routes>
          </MemoryRouter>
        </ToastProvider>
      </QueryClientProvider>,
    );

    // Form loads for default connection c1
    await screen.findByRole("heading", { name: "Edit Homebox 1" });

    // Click Delete, then Confirm
    fireEvent.click(screen.getByRole("button", { name: /^delete$/i }));
    fireEvent.click(screen.getByRole("button", { name: /^confirm$/i }));

    // Lands on /connections without a page reload
    const table = await screen.findByRole("table");
    expect(within(table).getByText("Homebox 2")).toBeInTheDocument();
    expect(within(table).queryByText("Homebox 1")).not.toBeInTheDocument();

    // Default connection control shows no default
    const select = (await screen.findByLabelText("default connection")) as HTMLSelectElement;
    expect(select.value).toBe("");
    expect(screen.queryByText(/unavailable/i)).not.toBeInTheDocument();
  });

  it("3.5 relays location.state.from onto Add connection and Edit links", async () => {
    fetchMock = stubFetch([
      { id: "c1", connector: "homebox", name: "Main", base_url: "http://hb", enabled: true, has_credential: true, transforms: [] },
    ]);
    vi.stubGlobal("fetch", fetchMock);

    // 1. With state: { from: "/connect" }
    let lastLocation: { pathname: string; state: unknown } | null = null;
    const getLastLocation = () => lastLocation;
    function LocationWatcher() {
      lastLocation = useLocation();
      return null;
    }

    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { unmount } = render(
      <QueryClientProvider client={qc}>
        <ToastProvider>
          <MemoryRouter initialEntries={[{ pathname: "/connections", state: { from: "/connect" } }]}>
            <ConnectionsList />
            <LocationWatcher />
          </MemoryRouter>
        </ToastProvider>
      </QueryClientProvider>,
    );

    const editLink = await screen.findByRole("link", { name: "Edit" });
    expect(editLink).toHaveAttribute("href", "/connections/c1");
    const addLink = screen.getByRole("link", { name: "Add connection" });

    // In React Router MemoryRouter, click the link to verify relayed state
    fireEvent.click(addLink);
    expect(getLastLocation()?.pathname).toBe("/connections/new");
    expect(getLastLocation()?.state).toEqual({ from: "/connect" });

    fireEvent.click(editLink);
    expect(getLastLocation()?.pathname).toBe("/connections/c1");
    expect(getLastLocation()?.state).toEqual({ from: "/connect" });

    unmount();

    // 2. Without state
    const qc2 = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc2}>
        <ToastProvider>
          <MemoryRouter initialEntries={["/connections"]}>
            <ConnectionsList />
            <LocationWatcher />
          </MemoryRouter>
        </ToastProvider>
      </QueryClientProvider>,
    );

    const addLink2 = await screen.findByRole("link", { name: "Add connection" });
    await screen.findByText("Main");
    fireEvent.click(addLink2);
    expect(getLastLocation()?.pathname).toBe("/connections/new");
    expect(getLastLocation()?.state == null).toBe(true);

    const editLink2 = screen.getByRole("link", { name: "Edit" });
    fireEvent.click(editLink2);
    expect(getLastLocation()?.pathname).toBe("/connections/c1");
    expect(getLastLocation()?.state == null).toBe(true);
  });
});
