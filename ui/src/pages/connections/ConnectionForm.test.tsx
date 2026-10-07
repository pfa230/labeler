import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Routes, Route, useNavigate, useLocation } from "react-router-dom";
import { ToastProvider } from "../../app/toast";
import { ConnectionForm } from "./ConnectionForm";
import { ConnectionsList } from "./ConnectionsList";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

type C = {
  id: string;
  connector: string;
  name: string;
  base_url: string;
  public_url?: string | null;
  has_credential: boolean;
};
type ConnectionBody = {
  connector?: string;
  name: string;
  base_url: string;
  public_url?: string;
  credential?: string;
};

const defaultConnections: C[] = [
  {
    id: "c1",
    connector: "homebox",
    name: "Home 1",
    base_url: "http://hb1.lan:7745",
    public_url: null,
    has_credential: true,
  },
  {
    id: "c2",
    connector: "homebox",
    name: "Home 2",
    base_url: "http://hb2.lan:7745",
    public_url: "https://hb2.example.com",
    has_credential: true,
  },
];

function stubFetch(initialConnections: C[] = defaultConnections) {
  let state: C[] = [...initialConnections];
  return vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (input, init) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.startsWith("/api/connections/") && method === "DELETE") {
      const id = decodeURIComponent(url.slice("/api/connections/".length));
      state = state.filter((c) => c.id !== id);
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/connections/") && method === "PUT") {
      const id = decodeURIComponent(url.slice("/api/connections/".length));
      const b = JSON.parse(init!.body as string) as ConnectionBody;
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
      const b = JSON.parse(init!.body as string) as ConnectionBody;
      const c: C = {
        id: "c_new",
        connector: b.connector!,
        name: b.name,
        base_url: b.base_url,
        public_url: b.public_url ?? null,
        has_credential: !!b.credential,
      };
      state = [...state, c];
      return json(c, 201);
    }
    if (url === "/api/connections" && method === "GET") return json(state);
    if (url === "/api/settings" && method === "GET") {
      return json({ default_connection_id: { value: null, is_default: true } });
    }
    throw new Error(`unexpected fetch: ${url} ${method}`);
  });
}

function NavigationTester() {
  const navigate = useNavigate();
  const location = useLocation();
  return (
    <div>
      <span data-testid="current-location">{location.pathname}</span>
      <button type="button" onClick={() => navigate("/connections/c1")}>Nav to C1</button>
      <button type="button" onClick={() => navigate("/connections/c2")}>Nav to C2</button>
      <button type="button" onClick={() => navigate("/connections/new")}>Nav to New</button>
      <button type="button" onClick={() => navigate("/other")}>Nav to Other</button>
    </div>
  );
}

function renderForm(initialPath: string, state?: unknown, qc?: QueryClient) {
  const client = qc ?? new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return {
    client,
    ...render(
      <QueryClientProvider client={client}>
        <ToastProvider>
          <MemoryRouter initialEntries={[{ pathname: initialPath, state }]}>
            <NavigationTester />
            <Routes>
              <Route path="/connections/new" element={<ConnectionForm />} />
              <Route path="/connections/:id" element={<ConnectionForm />} />
              <Route
                path="/connections"
                element={
                  <div data-testid="connections-page">
                    <ConnectionsList />
                  </div>
                }
              />
              <Route path="/connect" element={<div data-testid="connect-page">Connect Page</div>} />
              <Route path="/other" element={<div data-testid="other-page">Other Page</div>} />
            </Routes>
          </MemoryRouter>
        </ToastProvider>
      </QueryClientProvider>,
    ),
  };
}

let fetchMock: ReturnType<typeof stubFetch>;

describe("ConnectionForm", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => vi.unstubAllGlobals());

  describe("Form fields and creation (Task 7.1)", () => {
    it("creates a connection and never displays the credential", async () => {
      renderForm("/connections/new");
      expect(await screen.findByRole("heading", { name: "New connection" })).toBeInTheDocument();
      expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent?.trim())).toEqual(["Details"]);

      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
      fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
      fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      expect(await screen.findByTestId("connections-page")).toBeInTheDocument();
      expect(screen.queryByText("hb_secret")).not.toBeInTheDocument();
      const post = fetchMock.mock.calls.find(
        ([u, i]) => String(u) === "/api/connections" && (i?.method ?? "GET") === "POST",
      );
      expect(post).toBeTruthy();
      expect(JSON.parse(post![1]!.body as string).credential).toBe("hb_secret");
    });

    it("requires an api key when creating", async () => {
      renderForm("/connections/new");
      expect(await screen.findByRole("heading", { name: "New connection" })).toBeInTheDocument();

      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
      fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      expect(await screen.findByText(/api key is required/i)).toBeInTheDocument();
      const post = fetchMock.mock.calls.find(
        ([u, i]) => String(u) === "/api/connections" && (i?.method ?? "GET") === "POST",
      );
      expect(post).toBeUndefined();
    });

    it("setting a public URL: request body carries it", async () => {
      renderForm("/connections/new");
      await screen.findByRole("heading", { name: "New connection" });

      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
      fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
      fireEvent.change(screen.getByLabelText(/public url/i), { target: { value: "https://homebox.example.com" } });
      fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      await waitFor(() => {
        const post = fetchMock.mock.calls.find(
          ([u, i]) => String(u) === "/api/connections" && (i?.method ?? "GET") === "POST",
        );
        expect(post).toBeTruthy();
        expect(JSON.parse(post![1]!.body as string).public_url).toBe("https://homebox.example.com");
      });
    });

    it("clearing a public URL: the PUT body is exactly the name and base url, so omission clears it", async () => {
      renderForm("/connections/c2");
      await screen.findByRole("heading", { name: "Edit Home 2" });

      expect(screen.getByLabelText(/public url/i)).toHaveValue("https://hb2.example.com");
      fireEvent.change(screen.getByLabelText(/public url/i), { target: { value: "" } });
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      await waitFor(() => {
        const put = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "PUT");
        expect(put).toBeTruthy();
        expect(JSON.parse(put![1]!.body as string)).toEqual({ name: "Home 2", base_url: "http://hb2.lan:7745" });
      });
    });

    it("rejecting invalid public URL in form: shows error and sends no request", async () => {
      renderForm("/connections/new");
      await screen.findByRole("heading", { name: "New connection" });

      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
      fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
      fireEvent.change(screen.getByLabelText(/public url/i), { target: { value: "homebox.example.com" } });
      fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      expect(await screen.findByText(/public url must be a valid url/i)).toBeInTheDocument();
      const post = fetchMock.mock.calls.find(
        ([u, i]) => String(u) === "/api/connections" && (i?.method ?? "GET") === "POST",
      );
      expect(post).toBeUndefined();
    });

    it("creating with public url left empty: body has no public_url key and the form has no enabled checkbox", async () => {
      renderForm("/connections/new");
      await screen.findByRole("heading", { name: "New connection" });
      expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();

      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
      fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
      fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      await waitFor(() => {
        const post = fetchMock.mock.calls.find(
          ([u, i]) => String(u) === "/api/connections" && (i?.method ?? "GET") === "POST",
        );
        expect(post).toBeTruthy();
        expect("public_url" in JSON.parse(post![1]!.body as string)).toBe(false);
      });
    });

    it("editing with a blank api key omits credential from the PUT (keeps stored key)", async () => {
      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Renamed" } });
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      await waitFor(() => {
        const put = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "PUT");
        expect(put).toBeTruthy();
        const body = JSON.parse(put![1]!.body as string) as ConnectionBody;
        expect("credential" in body).toBe(false);
      });
    });
  });

  describe("Route shell, unknown id, load states, and delete confirmation (Task 7.4)", () => {
    it("cold deep link to /connections/:id resolves without prior visit to /connections", async () => {
      renderForm("/connections/c1");
      expect(await screen.findByRole("heading", { name: "Edit Home 1", level: 1 })).toBeInTheDocument();

      const headings = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent?.trim());
      expect(headings).toEqual(["Details"]);

      // Connector is fixed at creation and disabled
      const connectorSelect = screen.getByLabelText(/^connector$/i);
      expect(connectorSelect).toBeDisabled();
      expect(connectorSelect).toHaveValue("homebox");

      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Home 1");
      expect(screen.getByLabelText(/base url/i)).toHaveValue("http://hb1.lan:7745");
    });

    it("unknown id renders not-found state naming the id and linking to /connections", async () => {
      renderForm("/connections/nonexistent-id");
      expect(await screen.findByText(/Connection "nonexistent-id" not found\./i)).toBeInTheDocument();
      const link = screen.getByRole("link", { name: /Back to connections/i });
      expect(link).toHaveAttribute("href", "/connections");
      expect(screen.queryByRole("button", { name: /^save$/i })).not.toBeInTheDocument();
    });

    it("renders loading indicator while connections list is still pending", async () => {
      let release: (() => void) | undefined;
      const pendingGate = new Promise<void>((r) => { release = r; });
      fetchMock = vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/connections") {
          await pendingGate;
          return json(defaultConnections);
        }
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stubFetch>;
      vi.stubGlobal("fetch", fetchMock);

      renderForm("/connections/c1");
      expect(screen.getByText(/Loading connections\.\.\./i)).toBeInTheDocument();
      expect(screen.queryByRole("heading", { name: "Edit Home 1" })).not.toBeInTheDocument();
      expect(screen.queryByText(/Connection "c1" not found/i)).not.toBeInTheDocument();

      release?.();
      expect(await screen.findByRole("heading", { name: "Edit Home 1" })).toBeInTheDocument();
    });

    it("renders failure when connections list failed to load and does not report unknown id", async () => {
      fetchMock = vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/connections") return json({ error: "Server exploded" }, 500);
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stubFetch>;
      vi.stubGlobal("fetch", fetchMock);

      renderForm("/connections/c1");
      expect(await screen.findByText(/Failed to load connections\./i)).toBeInTheDocument();
      expect(screen.queryByText(/No connection with id/i)).not.toBeInTheDocument();
      expect(screen.queryByRole("heading", { name: "Edit Home 1" })).not.toBeInTheDocument();
    });

    it("a failed background refetch of connections does not destroy the open form's unsaved draft", async () => {
      let refetchFails = false;
      fetchMock = vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/connections") {
          if (refetchFails) {
            return json({ error: "temporary glitch" }, 500);
          }
          return json(defaultConnections);
        }
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stubFetch>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      renderForm("/connections/c1", undefined, qc);

      // Open /connections/c1 and wait for form to mount
      await screen.findByRole("heading", { name: "Edit Home 1" });
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Home 1");

      // Enter unsaved draft fields
      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Draft Name In Progress" } });
      fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "my-secret-key" } });
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Draft Name In Progress");
      expect(screen.getByLabelText(/api key/i)).toHaveValue("my-secret-key");

      // Next refetch of connections fails (e.g. background window focus or poll returns 500)
      refetchFails = true;
      await qc.refetchQueries({ queryKey: ["connections"] });

      // Form remains mounted; draft is preserved and not replaced by "Failed to load connections."
      expect(screen.queryByText(/Failed to load connections\./i)).not.toBeInTheDocument();
      expect(screen.getByRole("heading", { name: "Edit Home 1" })).toBeInTheDocument();
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Draft Name In Progress");
      expect(screen.getByLabelText(/api key/i)).toHaveValue("my-secret-key");

      // When connections refetch recovers, draft still remains intact
      refetchFails = false;
      await qc.refetchQueries({ queryKey: ["connections"] });
      expect(screen.getByRole("heading", { name: "Edit Home 1" })).toBeInTheDocument();
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Draft Name In Progress");
      expect(screen.getByLabelText(/api key/i)).toHaveValue("my-secret-key");
    });

    it("confirming step before a delete request: cancel aborts and confirm deletes and navigates to /connections", async () => {
      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });

      const deleteBtn = screen.getByRole("button", { name: /^delete$/i });
      fireEvent.click(deleteBtn);

      // Confirm step shown: Confirm button is present, Delete button is replaced
      expect(screen.getByRole("button", { name: /^confirm$/i })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /^delete$/i })).not.toBeInTheDocument();

      // Click Cancel on delete confirmation (the second Cancel button on page)
      const cancelButtons = screen.getAllByRole("button", { name: /^cancel$/i });
      fireEvent.click(cancelButtons[1]);
      expect(screen.queryByRole("button", { name: /^confirm$/i })).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: /^delete$/i })).toBeInTheDocument();
      let deleteCall = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "DELETE");
      expect(deleteCall).toBeUndefined();

      // Click Delete again, then click Confirm
      fireEvent.click(screen.getByRole("button", { name: /^delete$/i }));
      fireEvent.click(screen.getByRole("button", { name: /^confirm$/i }));

      await waitFor(() => {
        deleteCall = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "DELETE");
        expect(deleteCall).toBeTruthy();
        expect(screen.getByTestId("connections-page")).toBeInTheDocument();
      });
    });

    it("/connections/new offers no delete", async () => {
      renderForm("/connections/new");
      await screen.findByRole("heading", { name: "New connection" });
      expect(screen.queryByRole("button", { name: /delete/i })).not.toBeInTheDocument();
    });
  });

  describe("Editor identity across route switches (Task 7.5)", () => {
    it("navigating from /connections/c2 with unsaved edits back to /connections/c1 shows c1 stored values and no c2 draft; deferred c2 save changes nothing on screen", async () => {
      let resolvePutC2!: (res: Response) => void;
      const putC2Promise = new Promise<Response>((r) => { resolvePutC2 = r; });

      fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/connections" && method === "GET") return json(defaultConnections);
        if (url === "/api/connections/c2" && method === "PUT") return putC2Promise;
        if (url.startsWith("/api/connections/") && method === "PUT") {
          return json(defaultConnections[0]);
        }
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stubFetch>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      // Connect cached c2's schema earlier; the save must evict it.
      qc.setQueryData(["connector-schema", "c2"], { version: "homebox-1", resources: [], relationships: [] });
      renderForm("/connections/c2", undefined, qc);

      await screen.findByRole("heading", { name: "Edit Home 2" });
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Home 2");

      // Edit c2 fields
      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Draft Name C2" } });
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Draft Name C2");

      // Press save on c2 (held in flight)
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      // Navigate back to c1 before c2 PUT finishes
      fireEvent.click(screen.getByRole("button", { name: "Nav to C1" }));

      // c1 must render its own stored values with NO draft from c2
      expect(await screen.findByRole("heading", { name: "Edit Home 1" })).toBeInTheDocument();
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Home 1");
      expect(screen.queryByDisplayValue("Draft Name C2")).not.toBeInTheDocument();

      // Now resolve c2's save
      await act(async () => {
        resolvePutC2(json({ ...defaultConnections[1], name: "Draft Name C2" }));
      });

      // Verification: location is STILL c1, form on screen is STILL c1
      expect(screen.getByTestId("current-location")).toHaveTextContent("/connections/c1");
      expect(screen.getByRole("heading", { name: "Edit Home 1" })).toBeInTheDocument();
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Home 1");

      // Eviction: c2 save evicted ["connections"] and ["connector-schema", "c2"]
      expect(qc.getQueryData(["connections"])).toBeUndefined();
      expect(qc.getQueryData(["connector-schema", "c2"])).toBeUndefined();

      // Navigate back to c2: opening c2 again is a fresh editor showing stored values, draft gone
      fireEvent.click(screen.getByRole("button", { name: "Nav to C2" }));
      expect(await screen.findByRole("heading", { name: "Edit Home 2" })).toBeInTheDocument();
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Home 2");
    });

    it("reopening the same connection without changing id resets draft via location.key", async () => {
      fetchMock = stubFetch();
      vi.stubGlobal("fetch", fetchMock);

      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Home 1");

      // Edit c1 draft
      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Draft Name Same ID" } });
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Draft Name Same ID");

      // Re-navigate to the same connection id (/connections/c1), generating a fresh location.key
      fireEvent.click(screen.getByRole("button", { name: "Nav to C1" }));

      // Editor remounts fresh under the new location.key, discarding draft and restoring stored values
      expect(await screen.findByRole("heading", { name: "Edit Home 1" })).toBeInTheDocument();
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Home 1");
      expect(screen.queryByDisplayValue("Draft Name Same ID")).not.toBeInTheDocument();
    });

    it("reopening /connections/new without changing id resets draft via location.key", async () => {
      fetchMock = stubFetch();
      vi.stubGlobal("fetch", fetchMock);

      renderForm("/connections/new");
      await screen.findByRole("heading", { name: "New connection" });

      // Enter draft input
      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Draft New Connection" } });
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("Draft New Connection");

      // Re-navigate to /connections/new (id is still undefined/"new", but location.key changed)
      fireEvent.click(screen.getByRole("button", { name: "Nav to New" }));

      // Draft is discarded because location.key changed
      expect(await screen.findByRole("heading", { name: "New connection" })).toBeInTheDocument();
      expect(screen.getByLabelText(/^name$/i)).toHaveValue("");
    });
  });

  describe("Return paths (Task 7.6)", () => {
    it("save from Connect's call to action returns to /connect", async () => {
      renderForm("/connections/new", { from: "/connect" });
      await screen.findByRole("heading", { name: "New connection" });

      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
      fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
      fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "secret" } });
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      expect(await screen.findByTestId("connect-page")).toBeInTheDocument();
    });

    it("save from Connect by way of the list returns to /connect", async () => {
      renderForm("/connections", { from: "/connect" });
      const editLinks = await screen.findAllByRole("link", { name: "Edit" });
      expect(editLinks[0]).toHaveAttribute("href", "/connections/c1");
      fireEvent.click(editLinks[0]);

      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
      expect(await screen.findByTestId("connect-page")).toBeInTheDocument();
    });

    it("cancel from Connect by way of the list returns to /connect without sending a save request", async () => {
      renderForm("/connections", { from: "/connect" });
      const editLinks = await screen.findAllByRole("link", { name: "Edit" });
      expect(editLinks[0]).toHaveAttribute("href", "/connections/c1");
      fireEvent.click(editLinks[0]);

      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(screen.getByRole("button", { name: /^cancel$/i }));
      expect(await screen.findByTestId("connect-page")).toBeInTheDocument();
      const put = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "PUT");
      expect(put).toBeUndefined();
    });

    it("save from the list reached on its own returns to /connections", async () => {
      renderForm("/connections");
      const editLinks = await screen.findAllByRole("link", { name: "Edit" });
      expect(editLinks[0]).toHaveAttribute("href", "/connections/c1");
      fireEvent.click(editLinks[0]);

      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
      expect(await screen.findByTestId("connections-page")).toBeInTheDocument();
    });

    it("cancel from the list reached on its own returns to /connections", async () => {
      renderForm("/connections");
      const editLinks = await screen.findAllByRole("link", { name: "Edit" });
      expect(editLinks[0]).toHaveAttribute("href", "/connections/c1");
      fireEvent.click(editLinks[0]);

      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(screen.getByRole("button", { name: /^cancel$/i }));
      expect(await screen.findByTestId("connections-page")).toBeInTheDocument();
    });

    it("save from a cold load without state returns to /connections", async () => {
      renderForm("/connections/new");
      await screen.findByRole("heading", { name: "New connection" });

      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
      fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
      fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "secret" } });
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      expect(await screen.findByTestId("connections-page")).toBeInTheDocument();
    });

    it("a recorded origin other than Connect or /connections falls back to /connections", async () => {
      for (const origin of ["https://evil.example.com", "/settings"]) {
        const { unmount } = renderForm("/connections/c1", { from: origin });
        await screen.findByRole("heading", { name: "Edit Home 1" });

        fireEvent.click(screen.getByRole("button", { name: /^cancel$/i }));
        await waitFor(() => expect(screen.getByTestId("current-location")).toHaveTextContent(/^\/connections$/));
        unmount();
      }
    });

    it("recorded protocol-relative origin //evil.com falls back to /connections", async () => {
      renderForm("/connections/c1", { from: "//evil.com" });
      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
      expect(await screen.findByTestId("connections-page")).toBeInTheDocument();
    });

    it("recorded backslash origin /\\evil.com falls back to /connections", async () => {
      renderForm("/connections/c1", { from: "/\\evil.com" });
      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
      expect(await screen.findByTestId("connections-page")).toBeInTheDocument();
    });

    it("recorded whitespace/control evasion origins fall back to /connections", async () => {
      for (const origin of ["/\t/evil.com", "/\n/evil.com", "/\r/evil.com"]) {
        const { unmount } = renderForm("/connections/c1", { from: origin });
        await screen.findByRole("heading", { name: "Edit Home 1" });

        fireEvent.click(screen.getByRole("button", { name: /^cancel$/i }));
        expect(await screen.findByTestId("connections-page")).toBeInTheDocument();
        unmount();
      }
    });

    it("deleting ignores the recorded origin and always returns to /connections", async () => {
      renderForm("/connections/c1", { from: "/connect" });
      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(screen.getByRole("button", { name: /^delete$/i }));
      fireEvent.click(screen.getByRole("button", { name: /^confirm$/i }));

      expect(await screen.findByTestId("connections-page")).toBeInTheDocument();
      expect(screen.queryByTestId("connect-page")).not.toBeInTheDocument();
    });

    it("completion does not move an operator who has already left while cache maintenance still happens", async () => {
      let resolveSave!: (res: Response) => void;
      const savePromise = new Promise<Response>((r) => { resolveSave = r; });

      fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/connections" && method === "GET") return json(defaultConnections);
        if (url === "/api/connections/c1" && method === "PUT") return savePromise;
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stubFetch>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      // Connect cached c1's schema earlier; the save must evict it.
      qc.setQueryData(["connector-schema", "c1"], { version: "homebox-1", resources: [], relationships: [] });
      renderForm("/connections/c1", { from: "/connect" }, qc);
      await screen.findByRole("heading", { name: "Edit Home 1" });

      // Click save
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      // Navigate to /other before save resolves
      fireEvent.click(screen.getByRole("button", { name: "Nav to Other" }));
      expect(screen.getByTestId("other-page")).toBeInTheDocument();

      // Resolve save
      await act(async () => {
        resolveSave(json(defaultConnections[0]));
      });

      // Still on /other! Not redirected to /connect or /connections
      expect(screen.getByTestId("other-page")).toBeInTheDocument();
      expect(screen.getByTestId("current-location")).toHaveTextContent("/other");

      // Query cache was still evicted
      expect(qc.getQueryData(["connections"])).toBeUndefined();
      expect(qc.getQueryData(["connector-schema", "c1"])).toBeUndefined();
    });
  });
});
