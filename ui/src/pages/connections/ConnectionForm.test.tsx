import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Routes, Route, useNavigate, useLocation } from "react-router-dom";
import { ToastProvider } from "../../app/toast";
import { ConnectionForm } from "./ConnectionForm";
import { ConnectionsList } from "./ConnectionsList";
import type {
  FieldTransform,
  ConnectorSchema,
  TransformPreviewRequest,
  TransformPreviewResponse,
} from "../../api/connectors";

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
  enabled?: boolean;
  transforms?: FieldTransform[];
};

const defaultSchema: ConnectorSchema = {
  version: "1.0",
  resources: [
    {
      id: "entities",
      label: "Entities",
      view: "table",
      dynamic_source_prefix: "custom:",
      fields_incomplete: false,
      columns: [
        { key: "location", label: "Location", ty: "text", tier: "cheap", multi_valued: false, transform_source: true },
        { key: "name", label: "Name", ty: "text", tier: "cheap", multi_valued: false, transform_source: true },
        { key: "item_url", label: "Item URL", ty: "text", tier: "derived", multi_valued: false, transform_source: true },
        { key: "tags", label: "Tags", ty: "text", tier: "cheap", multi_valued: true, transform_source: false },
        { key: "location_id", label: "Location ID", ty: "text", tier: "derived", multi_valued: false, transform_source: false },
      ],
      filters: [],
    },
    {
      id: "locations",
      label: "Locations",
      view: "tree",
      dynamic_source_prefix: null,
      fields_incomplete: false,
      columns: [
        { key: "name", label: "Name", ty: "text", tier: "cheap", multi_valued: false, transform_source: true },
        { key: "location_url", label: "Location URL", ty: "text", tier: "derived", multi_valued: false, transform_source: true },
      ],
      filters: [],
    },
  ],
  relationships: [],
};

const defaultConnections: C[] = [
  {
    id: "c1",
    connector: "homebox",
    name: "Home 1",
    base_url: "http://hb1.lan:7745",
    public_url: null,
    enabled: true,
    has_credential: true,
    transforms: [],
  },
  {
    id: "c2",
    connector: "homebox",
    name: "Home 2",
    base_url: "http://hb2.lan:7745",
    public_url: "https://hb2.example.com",
    enabled: true,
    has_credential: true,
    transforms: [],
  },
];

function stubFetch(
  initialConnections: C[] = defaultConnections,
  initialSchemas: Record<string, ConnectorSchema> = {},
) {
  let state: C[] = [...initialConnections];
  return vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (input, init) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.startsWith("/api/connections/") && url.endsWith("/schema") && method === "GET") {
      const id = decodeURIComponent(url.slice("/api/connections/".length, url.length - "/schema".length));
      return json(initialSchemas[id] ?? defaultSchema);
    }
    if (url.startsWith("/api/connections/") && url.endsWith("/transforms/preview") && method === "POST") {
      const b = JSON.parse(init!.body as string) as TransformPreviewRequest;
      const ruleIdx = b.rule;
      const t = b.transforms[ruleIdx];
      const resp: TransformPreviewResponse = {
        rule: ruleIdx,
        resource: t?.resource ?? "entities",
        source: t?.source ?? "location",
        row_count: 2,
        matched_count: 1,
        rows: [
          {
            id: { resource: t?.resource ?? "entities", key: "row-1" },
            source_value: "BOX.123 | Motorcycle parts",
            matched: true,
            value_truncated: false,
            derived: { loc_id: "BOX.123", loc_name: "Motorcycle parts" },
          },
          {
            id: { resource: t?.resource ?? "entities", key: "row-2" },
            source_value: "invalid-format",
            matched: false,
            value_truncated: false,
          },
        ],
      };
      return json(resp);
    }
    if (url.startsWith("/api/connections/") && method === "DELETE") {
      const id = decodeURIComponent(url.slice("/api/connections/".length));
      state = state.filter((c) => c.id !== id);
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/connections/") && method === "PUT") {
      const id = decodeURIComponent(url.slice("/api/connections/".length));
      const b = JSON.parse(init!.body as string) as ConnectionInputBody;
      state = state.map((c) =>
        c.id === id
          ? {
              ...c,
              name: b.name,
              base_url: b.base_url,
              public_url: "public_url" in b ? b.public_url : c.public_url,
              has_credential: c.has_credential || !!b.credential,
              transforms: b.transforms ?? c.transforms,
              enabled: b.enabled ?? c.enabled,
            }
          : c,
      );
      return json(state.find((c) => c.id === id)!);
    }
    if (url === "/api/connections" && method === "POST") {
      const b = JSON.parse(init!.body as string) as ConnectionInputBody;
      const c: C = {
        id: "c_new",
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
      expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent?.trim())).toEqual([
        "Details",
        "Field transforms",
      ]);

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

    it("create form renders no rule editor and its request carries no transforms", async () => {
      renderForm("/connections/new");
      expect(await screen.findByRole("heading", { name: "New connection" })).toBeInTheDocument();

      expect(screen.queryByRole("button", { name: /\+ add rule/i })).not.toBeInTheDocument();
      expect(screen.getByText(/Transform rules can be added after saving the connection/i)).toBeInTheDocument();

      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "NewConn" } });
      fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://new.lan:7745" } });
      fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "secret123" } });
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      await waitFor(() => {
        const post = fetchMock.mock.calls.find(
          ([u, i]) => String(u) === "/api/connections" && (i?.method ?? "GET") === "POST",
        );
        expect(post).toBeTruthy();
        const body = JSON.parse(post![1]!.body as string) as ConnectionInputBody;
        expect("transforms" in body).toBe(false);
      });
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

    it("clearing a public URL: edit a connection that has one, empty the field, save asserts body carries public_url: null", async () => {
      renderForm("/connections/c2");
      await screen.findByRole("heading", { name: "Edit Home 2" });

      expect(screen.getByLabelText(/public url/i)).toHaveValue("https://hb2.example.com");
      fireEvent.change(screen.getByLabelText(/public url/i), { target: { value: "" } });
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      await waitFor(() => {
        const put = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "PUT");
        expect(put).toBeTruthy();
        const body = JSON.parse(put![1]!.body as string) as ConnectionInputBody;
        expect(body.public_url).toBeNull();
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

    it("creating with public url left empty: body carries public_url: null", async () => {
      renderForm("/connections/new");
      await screen.findByRole("heading", { name: "New connection" });

      fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: "Home" } });
      fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
      fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });
      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

      await waitFor(() => {
        const post = fetchMock.mock.calls.find(
          ([u, i]) => String(u) === "/api/connections" && (i?.method ?? "GET") === "POST",
        );
        expect(post).toBeTruthy();
        expect(JSON.parse(post![1]!.body as string).public_url).toBeNull();
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
        const body = JSON.parse(put![1]!.body as string) as ConnectionInputBody;
        expect("credential" in body).toBe(false);
      });
    });
  });

  describe("Transform rules and schema behavior (Task 7.1)", () => {
    it("rules round-trip through save", async () => {
      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(await screen.findByRole("button", { name: /\+ add rule/i }));
      fireEvent.change(screen.getByLabelText(/rule 0 source/i), { target: { value: "location" } });
      fireEvent.change(screen.getByLabelText(/rule 0 pattern/i), {
        target: { value: "^(?<loc_id>[^|]+)\\|(?<loc_name>.*)$" },
      });

      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
      await waitFor(() => {
        const put = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "PUT");
        expect(put).toBeTruthy();
        const body = JSON.parse(put![1]!.body as string) as ConnectionInputBody;
        expect(body.transforms).toEqual([
          {
            resource: "entities",
            source: "location",
            pattern: "^(?<loc_id>[^|]+)\\|(?<loc_name>.*)$",
          },
        ]);
      });
    });

    it("shows server transform error on the right rule", async () => {
      vi.stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
          const url = typeof input === "string" ? input : input.toString();
          const method = (init?.method ?? "GET").toUpperCase();
          if (url === "/api/connections/c1/schema") return json(defaultSchema);
          if (url.startsWith("/api/connections/c1") && method === "PUT") {
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
          if (url === "/api/connections") return json(defaultConnections);
          throw new Error(`unexpected fetch: ${url}`);
        }),
      );

      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });

      // Add rule 0 (valid)
      fireEvent.click(await screen.findByRole("button", { name: /\+ add rule/i }));
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

      expect(
        await screen.findByText("pattern must declare at least one named capture group"),
      ).toBeInTheDocument();
    });

    it("resource select offers exactly the schema's resource ids", async () => {
      const customSchema: ConnectorSchema = {
        version: "1.0",
        resources: [
          {
            id: "items",
            label: "Items",
            view: "table",
            dynamic_source_prefix: null,
            fields_incomplete: false,
            columns: [{ key: "name", label: "Name", ty: "text", tier: "cheap", multi_valued: false, transform_source: true }],
            filters: [],
          },
          {
            id: "categories",
            label: "Categories",
            view: "tree",
            dynamic_source_prefix: null,
            fields_incomplete: false,
            columns: [{ key: "title", label: "Title", ty: "text", tier: "cheap", multi_valued: false, transform_source: true }],
            filters: [],
          },
        ],
        relationships: [],
      };
      fetchMock = stubFetch(defaultConnections, { c1: customSchema });
      vi.stubGlobal("fetch", fetchMock);

      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(await screen.findByRole("button", { name: /\+ add rule/i }));
      const resourceSelect = screen.getByLabelText(/rule 0 resource/i) as HTMLSelectElement;
      const options = Array.from(resourceSelect.options).map((o) => o.value);
      expect(options).toEqual(["items", "categories"]);
    });

    it("source control offers transform_source columns, excludes multi-valued / derived, and offers custom by-name only where dynamic_source_prefix exists", async () => {
      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(await screen.findByRole("button", { name: /\+ add rule/i }));
      const sourceSelect = screen.getByLabelText(/rule 0 source/i) as HTMLSelectElement;
      const entityOptions = Array.from(sourceSelect.options).map((o) => o.value);
      expect(entityOptions).toContain("location");
      expect(entityOptions).toContain("name");
      expect(entityOptions).toContain("item_url");
      expect(entityOptions).toContain("__custom_by_name__");
      expect(entityOptions).not.toContain("tags");
      expect(entityOptions).not.toContain("location_id");

      fireEvent.change(screen.getByLabelText(/rule 0 resource/i), { target: { value: "locations" } });
      const locSourceSelect = screen.getByLabelText(/rule 0 source/i) as HTMLSelectElement;
      const locOptions = Array.from(locSourceSelect.options).map((o) => o.value);
      expect(locOptions).toContain("name");
      expect(locOptions).toContain("location_url");
      expect(locOptions).not.toContain("__custom_by_name__");
    });

    it("naming a field under prefix composes source as prefix followed by name", async () => {
      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(await screen.findByRole("button", { name: /\+ add rule/i }));
      fireEvent.change(screen.getByLabelText(/rule 0 source/i), { target: { value: "__custom_by_name__" } });
      fireEvent.change(screen.getByLabelText(/rule 0 source name/i), { target: { value: "Internal SKU" } });
      fireEvent.change(screen.getByLabelText(/rule 0 pattern/i), { target: { value: "^(?<sku>.*)$" } });

      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
      await waitFor(() => {
        const put = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "PUT");
        expect(put).toBeTruthy();
        const body = JSON.parse(put![1]!.body as string) as ConnectionInputBody;
        expect(body.transforms).toEqual([
          { resource: "entities", source: "custom:Internal SKU", pattern: "^(?<sku>.*)$" },
        ]);
      });
    });

    it("discloses possibly short field list when fields_incomplete is true", async () => {
      const incompleteSchema: ConnectorSchema = {
        ...defaultSchema,
        resources: [
          { ...defaultSchema.resources[0], fields_incomplete: true },
          defaultSchema.resources[1],
        ],
      };
      fetchMock = stubFetch(defaultConnections, { c1: incompleteSchema });
      vi.stubGlobal("fetch", fetchMock);

      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(await screen.findByRole("button", { name: /\+ add rule/i }));
      expect(await screen.findByText(/Field list may be short/i)).toBeInTheDocument();
    });

    it("preview panel renders matched count and non-matching row", async () => {
      fetchMock = stubFetch([
        {
          ...defaultConnections[0],
          transforms: [{ resource: "entities", source: "location", pattern: "^(?<id>[^|]+)\\|(?<name>.*)$" }],
        },
      ]);
      vi.stubGlobal("fetch", fetchMock);

      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(await screen.findByRole("button", { name: /preview rule 0/i }));
      expect(await screen.findByText(/Matched 1 of 2 rows/i)).toBeInTheDocument();
      expect(screen.getByText(/source: "BOX.123 \| Motorcycle parts"/i)).toBeInTheDocument();
      expect(screen.getByText(/loc_id="BOX.123", loc_name="Motorcycle parts"/i)).toBeInTheDocument();
      expect(screen.getByText(/source: "invalid-format"/i)).toBeInTheDocument();
      expect(screen.getByText(/Did not match/i)).toBeInTheDocument();
    });

    it("changing base url discards preview, shows stored rules read-only, offers no preview, and save sends no transforms", async () => {
      fetchMock = stubFetch([
        {
          ...defaultConnections[0],
          transforms: [{ resource: "entities", source: "location", pattern: "^(?<id>[^|]+)\\|(?<name>.*)$" }],
        },
      ]);
      vi.stubGlobal("fetch", fetchMock);

      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(await screen.findByRole("button", { name: /preview rule 0/i }));
      expect(await screen.findByText(/Matched 1 of 2 rows/i)).toBeInTheDocument();

      fireEvent.change(screen.getByLabelText(/rule 0 pattern/i), { target: { value: "edited_pattern" } });
      fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb-updated.lan" } });

      expect(screen.queryByText(/Matched 1 of 2 rows/i)).not.toBeInTheDocument();
      expect(screen.getByText(/Connection details must be saved first/i)).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /preview rule 0/i })).not.toBeInTheDocument();

      expect(screen.getByText(/pattern:\s*\^\(\?<id>\[\^\|\]\+\)\\\|\(\?<name>\.\*\)\$/)).toBeInTheDocument();
      expect(screen.queryByText(/pattern:\s*edited_pattern/)).not.toBeInTheDocument();

      fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
      await waitFor(() => {
        const put = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "PUT");
        expect(put).toBeTruthy();
        const body = JSON.parse(put![1]!.body as string) as ConnectionInputBody;
        expect("transforms" in body).toBe(false);
      });
    });

    it("in-flight response stays discarded when a rule is edited and restored before it resolves", async () => {
      let resolvePreview: ((res: Response) => void) | undefined;
      const previewGate = new Promise<Response>((r) => { resolvePreview = r; });

      fetchMock = vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/connections") {
          return json([{ ...defaultConnections[0], transforms: [{ resource: "entities", source: "location", pattern: "original_pattern" }] }]);
        }
        if (url === "/api/connections/c1/schema") return json(defaultSchema);
        if (url === "/api/connections/c1/transforms/preview") return previewGate;
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stubFetch>;
      vi.stubGlobal("fetch", fetchMock);

      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });
      const previewBtn = await screen.findByRole("button", { name: /preview rule 0/i });

      fireEvent.click(previewBtn);
      expect(previewBtn).toHaveTextContent("Previewing...");

      fireEvent.change(screen.getByLabelText(/rule 0 pattern/i), { target: { value: "modified_pattern" } });
      expect(previewBtn).toHaveTextContent("Preview");

      fireEvent.change(screen.getByLabelText(/rule 0 pattern/i), { target: { value: "original_pattern" } });

      resolvePreview!(json({
        rule: 0,
        resource: "entities",
        source: "location",
        row_count: 2,
        matched_count: 2,
        rows: [{ id: { resource: "entities", key: "1" }, source_value: "val", matched: true, value_truncated: false }],
      }));

      await new Promise((r) => setTimeout(r, 50));
      expect(screen.queryByText(/Matched 2 of 2 rows/i)).not.toBeInTheDocument();
      expect(previewBtn).toHaveTextContent("Preview");
    });

    it("earlier of two overlapping preview requests does not overwrite later response", async () => {
      let resolveReq1: ((res: Response) => void) | undefined;
      let resolveReq2: ((res: Response) => void) | undefined;
      let callCount = 0;

      fetchMock = vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/connections") {
          return json([{ ...defaultConnections[0], transforms: [{ resource: "entities", source: "location", pattern: "pattern" }] }]);
        }
        if (url === "/api/connections/c1/schema") return json(defaultSchema);
        if (url === "/api/connections/c1/transforms/preview") {
          callCount += 1;
          if (callCount === 1) return new Promise<Response>((r) => { resolveReq1 = r; });
          return new Promise<Response>((r) => { resolveReq2 = r; });
        }
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stubFetch>;
      vi.stubGlobal("fetch", fetchMock);

      renderForm("/connections/c1");
      await screen.findByRole("heading", { name: "Edit Home 1" });
      const previewBtn = await screen.findByRole("button", { name: /preview rule 0/i });

      fireEvent.click(previewBtn);
      fireEvent.click(previewBtn);

      resolveReq2!(json({
        rule: 0,
        resource: "entities",
        source: "location",
        row_count: 5,
        matched_count: 5,
        rows: [{ id: { resource: "entities", key: "1" }, source_value: "val2", matched: true, value_truncated: false }],
      }));

      expect(await screen.findByText(/Matched 5 of 5 rows/i)).toBeInTheDocument();

      resolveReq1!(json({
        rule: 0,
        resource: "entities",
        source: "location",
        row_count: 1,
        matched_count: 1,
        rows: [{ id: { resource: "entities", key: "1" }, source_value: "val1", matched: true, value_truncated: false }],
      }));

      await new Promise((r) => setTimeout(r, 50));
      expect(screen.getByText(/Matched 5 of 5 rows/i)).toBeInTheDocument();
      expect(screen.queryByText(/Matched 1 of 1 rows/i)).not.toBeInTheDocument();
    });
  });

  describe("Route shell, unknown id, load states, and delete confirmation (Task 7.4)", () => {
    it("cold deep link to /connections/:id resolves without prior visit to /connections", async () => {
      renderForm("/connections/c1");
      expect(await screen.findByRole("heading", { name: "Edit Home 1", level: 1 })).toBeInTheDocument();

      // Form renders in titled sections: Details, then Field transforms
      const headings = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent?.trim());
      expect(headings).toEqual(["Details", "Field transforms"]);

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
        if (url === "/api/connections/c1/schema") return json(defaultSchema);
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
        if (url.startsWith("/api/connections/") && url.endsWith("/schema")) {
          return json(defaultSchema);
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
        if (url.startsWith("/api/connections/") && url.endsWith("/schema")) return json(defaultSchema);
        if (url === "/api/connections/c2" && method === "PUT") return putC2Promise;
        if (url.startsWith("/api/connections/") && method === "PUT") {
          return json(defaultConnections[0]);
        }
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stubFetch>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
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

    it("recorded origin that is not an in-app path falls back to /connections", async () => {
      renderForm("/connections/c1", { from: "https://evil.example.com" });
      await screen.findByRole("heading", { name: "Edit Home 1" });

      fireEvent.click(screen.getByRole("button", { name: /^cancel$/i }));
      expect(await screen.findByTestId("connections-page")).toBeInTheDocument();
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
        if (url === "/api/connections/c1/schema") return json(defaultSchema);
        if (url === "/api/connections/c1" && method === "PUT") return savePromise;
        throw new Error(`unexpected fetch: ${url}`);
      }) as ReturnType<typeof stubFetch>;
      vi.stubGlobal("fetch", fetchMock);

      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
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
