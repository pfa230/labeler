import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { ConnectionsSection } from "./ConnectionsSection";
import type { FieldTransform } from "../../api/connectors";

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
  transforms: FieldTransform[];
};
type ConnectionInputBody = {
  connector: string;
  name: string;
  base_url: string;
  public_url?: string | null;
  credential?: string;
  transforms?: FieldTransform[];
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
    if (url.startsWith("/api/connections/") && method === "PUT") {
      const id = decodeURIComponent(url.slice("/api/connections/".length));
      const b = JSON.parse(init!.body as string) as ConnectionInputBody;
      // blank/omitted credential keeps the stored key (mirrors the backend semantics)
      state = state.map((c) =>
        c.id === id
          ? {
              ...c,
              name: b.name,
              base_url: b.base_url,
              public_url: "public_url" in b ? b.public_url : c.public_url,
              has_credential: c.has_credential || !!b.credential,
              transforms: b.transforms ?? c.transforms,
            }
          : c,
      );
      return json(state.find((c) => c.id === id)!);
    }
    if (url.startsWith("/api/connections") && method === "POST") {
      const b = JSON.parse(init!.body as string) as ConnectionInputBody;
      const c: C = {
        id: "id1",
        connector: b.connector,
        name: b.name,
        base_url: b.base_url,
        public_url: b.public_url ?? null,
        enabled: true,
        has_credential: !!b.credential,
        transforms: b.transforms ?? [],
      };
      state = [...state, c];
      return json(c, 201);
    }
    if (url.startsWith("/api/connections")) return json(state);
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

function renderSection() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <ConnectionsSection />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

let fetchMock: ReturnType<typeof stubFetch>;
describe("ConnectionsSection", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => vi.unstubAllGlobals());

  it("creates a connection and never displays the credential", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("Home")).toBeInTheDocument();
    expect(screen.queryByText("hb_secret")).not.toBeInTheDocument();
    const post = fetchMock.mock.calls.find(
      ([u, i]) => String(u) === "/api/connections" && (i?.method ?? "GET") === "POST",
    );
    expect(JSON.parse(post![1]!.body as string).credential).toBe("hb_secret");
  });

  it("requires an api key when creating", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText(/api key is required/i)).toBeInTheDocument();
  });

  it("editing with a blank api key omits credential from the PUT (keeps the stored key)", async () => {
    renderSection();
    // seed a connection via create (with a key)
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("Home")).toBeInTheDocument();
    // edit: change only the name, leave the api key blank
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Renamed" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("Renamed")).toBeInTheDocument();
    const put = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "PUT");
    expect(put).toBeTruthy();
    const body = JSON.parse(put![1]!.body as string) as ConnectionInputBody;
    expect("credential" in body).toBe(false); // blank key MUST NOT be sent, so the backend keeps it
    // the stored key is preserved (still "set")
    expect(await screen.findByText("set")).toBeInTheDocument();
  });

  it("rules round-trip through save", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });

    // Add a rule
    fireEvent.click(screen.getByRole("button", { name: /\+ add rule/i }));
    fireEvent.change(screen.getByLabelText(/rule 0 source/i), { target: { value: "location" } });
    fireEvent.change(screen.getByLabelText(/rule 0 pattern/i), {
      target: { value: "^(?<loc_id>[^|]+)\\|(?<loc_name>.*)$" },
    });

    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("Home")).toBeInTheDocument();

    const post = fetchMock.mock.calls.find(
      ([u, i]) => String(u) === "/api/connections" && (i?.method ?? "GET") === "POST",
    );
    const body = JSON.parse(post![1]!.body as string) as ConnectionInputBody;
    expect(body.transforms).toEqual([
      {
        resource: "entities",
        source: "location",
        pattern: "^(?<loc_id>[^|]+)\\|(?<loc_name>.*)$",
      },
    ]);

    // Edit and add another rule, remove the first
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    expect(screen.getByLabelText(/rule 0 source/i)).toHaveValue("location");

    fireEvent.click(screen.getByRole("button", { name: /\+ add rule/i }));
    fireEvent.change(screen.getByLabelText(/rule 1 source/i), { target: { value: "name" } });
    fireEvent.change(screen.getByLabelText(/rule 1 pattern/i), {
      target: { value: "^(?<prefix>[A-Z]+)-(?<num>\\d+)$" },
    });

    // Remove first rule
    fireEvent.click(screen.getByRole("button", { name: /remove rule 0/i }));

    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("Home")).toBeInTheDocument();

    const put = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "PUT");
    const putBody = JSON.parse(put![1]!.body as string) as ConnectionInputBody;
    expect(putBody.transforms).toEqual([
      {
        resource: "entities",
        source: "name",
        pattern: "^(?<prefix>[A-Z]+)-(?<num>\\d+)$",
      },
    ]);
  });

  it("shows server transform error on the right rule", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = typeof input === "string" ? input : input.toString();
        const method = (init?.method ?? "GET").toUpperCase();
        if (url === "/api/connections" && method === "POST") {
          return json(
            {
              error: {
                code: "InvalidRequest",
                message: "rule 1: pattern must declare at least one named capture group",
                details: { reason: "connection_transform_invalid" },
              },
            },
            400,
          );
        }
        if (url === "/api/connections") return json([]);
        throw new Error(`unexpected fetch: ${url}`);
      }),
    );

    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });

    // Add rule 0 (valid)
    fireEvent.click(screen.getByRole("button", { name: /\+ add rule/i }));
    fireEvent.change(screen.getByLabelText(/rule 0 source/i), { target: { value: "location" } });
    fireEvent.change(screen.getByLabelText(/rule 0 pattern/i), {
      target: { value: "^(?<loc_id>.*)$" },
    });

    // Add rule 1 (invalid)
    fireEvent.click(screen.getByRole("button", { name: /\+ add rule/i }));
    fireEvent.change(screen.getByLabelText(/rule 1 source/i), { target: { value: "name" } });
    fireEvent.change(screen.getByLabelText(/rule 1 pattern/i), {
      target: { value: ".*" },
    });

    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

    // Rule 1 should show the error
    expect(
      await screen.findByText("pattern must declare at least one named capture group"),
    ).toBeInTheDocument();
  });

  it("setting a public URL: request body carries it and table row shows it", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
    fireEvent.change(screen.getByLabelText(/public url/i), { target: { value: "https://homebox.example.com" } });
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("Home")).toBeInTheDocument();
    expect(await screen.findByText("https://homebox.example.com")).toBeInTheDocument();
    const post = fetchMock.mock.calls.find(
      ([u, i]) => String(u) === "/api/connections" && (i?.method ?? "GET") === "POST",
    );
    expect(JSON.parse(post![1]!.body as string).public_url).toBe("https://homebox.example.com");
  });

  it("clearing a public URL: edit a connection that has one, empty the field, save, asserts body carries public_url: null and row shows -", async () => {
    renderSection();
    // seed connection with public_url
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
    fireEvent.change(screen.getByLabelText(/public url/i), { target: { value: "https://homebox.example.com" } });
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("https://homebox.example.com")).toBeInTheDocument();

    // edit: clear public url
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    expect(screen.getByLabelText(/public url/i)).toHaveValue("https://homebox.example.com");
    fireEvent.change(screen.getByLabelText(/public url/i), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("-")).toBeInTheDocument();
    expect(screen.queryByText("https://homebox.example.com")).not.toBeInTheDocument();

    const put = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "PUT");
    expect(put).toBeTruthy();
    const body = JSON.parse(put![1]!.body as string) as ConnectionInputBody;
    expect(body.public_url).toBeNull();
  });

  it("rejecting invalid public URL in form: shows error and sends no request", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
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

  it("creating with public url left empty: body carries public_url: null", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("Home")).toBeInTheDocument();
    expect(await screen.findByText("-")).toBeInTheDocument();
    const post = fetchMock.mock.calls.find(
      ([u, i]) => String(u) === "/api/connections" && (i?.method ?? "GET") === "POST",
    );
    expect(JSON.parse(post![1]!.body as string).public_url).toBeNull();
  });

  it("picking a default connection sends PUT with the id", async () => {
    fetchMock = stubFetch([
      { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: true, has_credential: true, transforms: [] },
    ]);
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const select = await screen.findByLabelText(/default connection/i);
    await waitFor(() => expect(select).toHaveValue(""));

    fireEvent.change(select, { target: { value: "c1" } });

    await waitFor(() => {
      const put = fetchMock.mock.calls.find(
        ([u, i]) => String(u) === "/api/settings/default_connection_id" && (i?.method ?? "GET") === "PUT",
      );
      expect(put).toBeTruthy();
      expect(JSON.parse(put![1]!.body as string)).toEqual({ value: "c1" });
    });
  });

  it("selecting no default sends DELETE", async () => {
    fetchMock = stubFetch(
      [
        { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: true, has_credential: true, transforms: [] },
      ],
      {
        default_connection_id: { value: "c1", is_default: false },
      },
    );
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const select = await screen.findByLabelText(/default connection/i);
    await waitFor(() => expect(select).toHaveValue("c1"));

    fireEvent.change(select, { target: { value: "" } });

    await waitFor(() => {
      const del = fetchMock.mock.calls.find(
        ([u, i]) => String(u) === "/api/settings/default_connection_id" && (i?.method ?? "GET") === "DELETE",
      );
      expect(del).toBeTruthy();
    });
  });

  it("shows no default choice when no default is stored", async () => {
    fetchMock = stubFetch([
      { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: true, has_credential: true, transforms: [] },
    ]);
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const select = await screen.findByLabelText(/default connection/i);
    await waitFor(() => expect(select).toHaveValue(""));
  });

  it("marks disabled connections in the control", async () => {
    fetchMock = stubFetch([
      { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: false, has_credential: true, transforms: [] },
    ]);
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const option = await screen.findByRole("option", { name: /Home \(c1\) \(disabled\)/i });
    expect(option).toBeInTheDocument();
  });

  it("distinguishes identically named connections by ID", async () => {
    fetchMock = stubFetch([
      { id: "c1", connector: "homebox", name: "Homebox", base_url: "http://hb1.lan", enabled: true, has_credential: true, transforms: [] },
      { id: "c2", connector: "homebox", name: "Homebox", base_url: "http://hb2.lan", enabled: true, has_credential: true, transforms: [] },
    ]);
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    expect(await screen.findByRole("option", { name: "Homebox (c1)" })).toBeInTheDocument();
    expect(await screen.findByRole("option", { name: "Homebox (c2)" })).toBeInTheDocument();
  });

  it("shows unavailable state for dangling stored id and allows clearing it", async () => {
    fetchMock = stubFetch(
      [
        { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: true, has_credential: true, transforms: [] },
      ],
      {
        default_connection_id: { value: "dangling-conn-id", is_default: false },
      },
    );
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const option = await screen.findByRole("option", { name: "dangling-conn-id (unavailable)" });
    expect(option).toBeInTheDocument();
    const select = screen.getByLabelText(/default connection/i);
    await waitFor(() => expect(select).toHaveValue("dangling-conn-id"));

    fireEvent.change(select, { target: { value: "" } });
    await waitFor(() => {
      const del = fetchMock.mock.calls.find(
        ([u, i]) => String(u) === "/api/settings/default_connection_id" && (i?.method ?? "GET") === "DELETE",
      );
      expect(del).toBeTruthy();
    });
  });

  it("deleting the default connection clears the control without a reload", async () => {
    fetchMock = stubFetch(
      [
        { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: true, has_credential: true, transforms: [] },
      ],
      {
        default_connection_id: { value: "c1", is_default: false },
      },
    );
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const select = await screen.findByLabelText(/default connection/i);
    await waitFor(() => expect(select).toHaveValue("c1"));

    // Click delete connection row and confirm
    fireEvent.click(screen.getByRole("button", { name: /^delete$/i }));
    fireEvent.click(screen.getByRole("button", { name: /^confirm$/i }));

    // Wait for delete mutation & query invalidation
    await waitFor(() => expect(screen.getByLabelText(/default connection/i)).toHaveValue(""));
    expect(screen.queryByRole("option", { name: /Home \(c1\)/i })).not.toBeInTheDocument();
  });

  // "Unavailable" is a claim about the stored id naming no connection. Before the connections list
  // has answered, we do not know that. Saying it anyway invites the operator to clear a setting that
  // was never broken.
  it("does not call a stored default unavailable when the connections list failed to load", async () => {
    fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (input) => {
      const url = String(input);
      if (url === "/api/settings") return json({ default_connection_id: { value: "c1", is_default: false } });
      if (url === "/api/connections") return json({ error: "boom" }, 500);
      throw new Error(`unexpected fetch: ${url}`);
    }) as ReturnType<typeof stubFetch>;
    vi.stubGlobal("fetch", fetchMock);
    renderSection();

    await waitFor(() => expect(screen.getByText(/Failed to load connections/i)).toBeInTheDocument());
    const select = await screen.findByLabelText(/default connection/i);
    expect(screen.queryByText(/unavailable/i)).not.toBeInTheDocument();
    // The stored id is still reported truthfully, and cannot be acted on while the list is unknown.
    expect(select).toHaveValue("c1");
    expect(select).toBeDisabled();
  });

  it("does not call a stored default unavailable while the connections list is still loading", async () => {
    let release: (() => void) | undefined;
    const gate = new Promise<void>((r) => { release = r; });
    fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (input) => {
      const url = String(input);
      if (url === "/api/settings") return json({ default_connection_id: { value: "c1", is_default: false } });
      if (url === "/api/connections") {
        await gate;
        return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: true, has_credential: true, transforms: [] }]);
      }
      throw new Error(`unexpected fetch: ${url}`);
    }) as ReturnType<typeof stubFetch>;
    vi.stubGlobal("fetch", fetchMock);
    renderSection();

    const select = await screen.findByLabelText(/default connection/i);
    await waitFor(() => expect(select).toHaveValue("c1"));
    expect(screen.queryByText(/unavailable/i)).not.toBeInTheDocument();

    release?.();
    await waitFor(() => expect(screen.getByRole("option", { name: /Home \(c1\)/i })).toBeInTheDocument());
    expect(screen.queryByText(/unavailable/i)).not.toBeInTheDocument();
    expect(select).not.toBeDisabled();
  });
});
