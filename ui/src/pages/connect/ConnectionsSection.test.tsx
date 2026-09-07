import { useState } from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { ConnectionsSection } from "./ConnectionsSection";
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

function stubFetch(
  initialConnections: C[] = [],
  initialSettings: Record<string, { value: unknown; is_default: boolean }> = {
    default_connection_id: { value: null, is_default: true },
  },
  initialSchemas: Record<string, ConnectorSchema> = {},
) {
  let state: C[] = [...initialConnections];
  let settingsState = { ...initialSettings };
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

function renderSection(client?: QueryClient) {
  const qc = client ?? new QueryClient({ defaultOptions: { queries: { retry: false } } });
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
    fetchMock = stubFetch([
      { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan:7745", enabled: true, has_credential: true, transforms: [] },
    ]);
    vi.stubGlobal("fetch", fetchMock);
    renderSection();

    // Edit and add a rule
    fireEvent.click(await screen.findByRole("button", { name: /^edit$/i }));
    fireEvent.click(await screen.findByRole("button", { name: /\+ add rule/i }));
    fireEvent.change(screen.getByLabelText(/rule 0 source/i), { target: { value: "location" } });
    fireEvent.change(screen.getByLabelText(/rule 0 pattern/i), {
      target: { value: "^(?<loc_id>[^|]+)\\|(?<loc_name>.*)$" },
    });

    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("Home")).toBeInTheDocument();

    const put = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "PUT");
    const body = JSON.parse(put![1]!.body as string) as ConnectionInputBody;
    expect(body.transforms).toEqual([
      {
        resource: "entities",
        source: "location",
        pattern: "^(?<loc_id>[^|]+)\\|(?<loc_name>.*)$",
      },
    ]);

    // Edit and add another rule, remove the first
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    await screen.findByDisplayValue("^(?<loc_id>[^|]+)\\|(?<loc_name>.*)$");

    fireEvent.click(screen.getByRole("button", { name: /\+ add rule/i }));
    fireEvent.change(screen.getByLabelText(/rule 1 source/i), { target: { value: "name" } });
    fireEvent.change(screen.getByLabelText(/rule 1 pattern/i), {
      target: { value: "^(?<prefix>[A-Z]+)-(?<num>\\d+)$" },
    });

    // Remove first rule
    fireEvent.click(screen.getByRole("button", { name: /remove rule 0/i }));

    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("Home")).toBeInTheDocument();

    const secondPut = fetchMock.mock.calls.filter(([, i]) => (i?.method ?? "GET") === "PUT")[1];
    const putBody = JSON.parse(secondPut![1]!.body as string) as ConnectionInputBody;
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
        if (url === "/api/connections") {
          return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan:7745", enabled: true, has_credential: true, transforms: [] }]);
        }
        if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
        throw new Error(`unexpected fetch: ${url}`);
      }),
    );

    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /^edit$/i }));

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

    // Rule 1 should show the error
    expect(
      await screen.findByText("pattern must declare at least one named capture group"),
    ).toBeInTheDocument();
  });

  it("7.1 The resource select offers exactly the schema's resource ids", async () => {
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
    fetchMock = stubFetch(
      [{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: true, has_credential: true, transforms: [] }],
      undefined,
      { c1: customSchema },
    );
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /^edit$/i }));
    fireEvent.click(await screen.findByRole("button", { name: /\+ add rule/i }));
    const resourceSelect = screen.getByLabelText(/rule 0 resource/i) as HTMLSelectElement;
    const options = Array.from(resourceSelect.options).map((o) => o.value);
    expect(options).toEqual(["items", "categories"]);
  });

  it("7.2 The source control offers the transform_source columns, offers neither a multi-valued nor a transform-derived column, and offers the by-name choice only where a dynamic_source_prefix exists", async () => {
    fetchMock = stubFetch([
      { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: true, has_credential: true, transforms: [] },
    ]);
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /^edit$/i }));
    fireEvent.click(await screen.findByRole("button", { name: /\+ add rule/i }));

    // On entities resource (has prefix "custom:"):
    const sourceSelect = screen.getByLabelText(/rule 0 source/i) as HTMLSelectElement;
    const entityOptions = Array.from(sourceSelect.options).map((o) => o.value);
    expect(entityOptions).toContain("location");
    expect(entityOptions).toContain("name");
    expect(entityOptions).toContain("item_url");
    expect(entityOptions).toContain("__custom_by_name__");
    expect(entityOptions).not.toContain("tags");
    expect(entityOptions).not.toContain("location_id");

    // Switch to locations resource (dynamic_source_prefix is null):
    fireEvent.change(screen.getByLabelText(/rule 0 resource/i), { target: { value: "locations" } });
    const locSourceSelect = screen.getByLabelText(/rule 0 source/i) as HTMLSelectElement;
    const locOptions = Array.from(locSourceSelect.options).map((o) => o.value);
    expect(locOptions).toContain("name");
    expect(locOptions).toContain("location_url");
    expect(locOptions).not.toContain("__custom_by_name__");
  });

  it("7.3 Naming a field under the prefix composes the rule's source as prefix followed by name", async () => {
    fetchMock = stubFetch([
      { id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: true, has_credential: true, transforms: [] },
    ]);
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /^edit$/i }));
    fireEvent.click(await screen.findByRole("button", { name: /\+ add rule/i }));

    // Select custom prefix
    fireEvent.change(screen.getByLabelText(/rule 0 source/i), { target: { value: "__custom_by_name__" } });
    // Type field name
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

  it("7.4 A resource carrying fields_incomplete true is disclosed as possibly short", async () => {
    const incompleteSchema: ConnectorSchema = {
      ...defaultSchema,
      resources: [
        {
          ...defaultSchema.resources[0],
          fields_incomplete: true,
        },
        defaultSchema.resources[1],
      ],
    };
    fetchMock = stubFetch(
      [{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: true, has_credential: true, transforms: [] }],
      undefined,
      { c1: incompleteSchema },
    );
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /^edit$/i }));
    fireEvent.click(await screen.findByRole("button", { name: /\+ add rule/i }));
    expect(await screen.findByText(/Field list may be short/i)).toBeInTheDocument();
  });

  it("7.5 The create form renders no rule editor and its request carries no transforms", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
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

  it("7.6 A preview panel renders the matched count and a non-matching row", async () => {
    fetchMock = stubFetch([
      {
        id: "c1",
        connector: "homebox",
        name: "Home",
        base_url: "http://hb.lan",
        enabled: true,
        has_credential: true,
        transforms: [{ resource: "entities", source: "location", pattern: "^(?<id>[^|]+)\\|(?<name>.*)$" }],
      },
    ]);
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /^edit$/i }));
    fireEvent.click(await screen.findByRole("button", { name: /preview rule 0/i }));

    expect(await screen.findByText(/Matched 1 of 2 rows/i)).toBeInTheDocument();
    expect(screen.getByText(/source: "BOX.123 \| Motorcycle parts"/i)).toBeInTheDocument();
    expect(screen.getByText(/loc_id="BOX.123", loc_name="Motorcycle parts"/i)).toBeInTheDocument();
    expect(screen.getByText(/source: "invalid-format"/i)).toBeInTheDocument();
    expect(screen.getByText(/Did not match/i)).toBeInTheDocument();
  });

  it("7.7 Changing the base url discards a displayed result, shows stored rules read-only, offers no preview, and makes the save send no transforms key", async () => {
    fetchMock = stubFetch([
      {
        id: "c1",
        connector: "homebox",
        name: "Home",
        base_url: "http://hb.lan",
        enabled: true,
        has_credential: true,
        transforms: [{ resource: "entities", source: "location", pattern: "^(?<id>[^|]+)\\|(?<name>.*)$" }],
      },
    ]);
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /^edit$/i }));
    fireEvent.click(await screen.findByRole("button", { name: /preview rule 0/i }));

    expect(await screen.findByText(/Matched 1 of 2 rows/i)).toBeInTheDocument();

    // Edit rule 0 pattern to a new value
    fireEvent.change(screen.getByLabelText(/rule 0 pattern/i), { target: { value: "edited_pattern" } });

    // Change base URL
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb-updated.lan" } });

    // Preview result is discarded
    expect(screen.queryByText(/Matched 1 of 2 rows/i)).not.toBeInTheDocument();
    // Editor is suspended
    expect(screen.getByText(/Connection details must be saved first/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /preview rule 0/i })).not.toBeInTheDocument();

    // Suspended editor renders stored rule (not the edited pattern)
    expect(screen.getByText(/pattern:\s*\^\(\?<id>\[\^\|\]\+\)\\\|\(\?<name>\.\*\)\$/)).toBeInTheDocument();
    expect(screen.queryByText(/pattern:\s*edited_pattern/)).not.toBeInTheDocument();

    // Save
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => {
      const put = fetchMock.mock.calls.find(([, i]) => (i?.method ?? "GET") === "PUT");
      expect(put).toBeTruthy();
      const body = JSON.parse(put![1]!.body as string) as ConnectionInputBody;
      expect("transforms" in body).toBe(false);
    });
  });

  it("7.8 An in-flight response stays discarded when a rule is edited and the exact original list is restored before it resolves, and previewing indicator does not stick", async () => {
    let resolvePreview: ((res: Response) => void) | undefined;
    const previewGate = new Promise<Response>((r) => { resolvePreview = r; });

    fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/connections") {
        return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: true, has_credential: true, transforms: [{ resource: "entities", source: "location", pattern: "original_pattern" }] }]);
      }
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      if (url === "/api/connections/c1/schema") return json(defaultSchema);
      if (url === "/api/connections/c1/transforms/preview") {
        return previewGate;
      }
      throw new Error(`unexpected fetch: ${url}`);
    }) as ReturnType<typeof stubFetch>;
    vi.stubGlobal("fetch", fetchMock);

    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /^edit$/i }));
    const previewBtn = await screen.findByRole("button", { name: /preview rule 0/i });

    // Issue preview
    fireEvent.click(previewBtn);
    expect(previewBtn).toHaveTextContent("Previewing...");

    // While in flight, edit rule pattern - previewing state immediately clears on edit
    fireEvent.change(screen.getByLabelText(/rule 0 pattern/i), { target: { value: "modified_pattern" } });
    expect(previewBtn).toHaveTextContent("Preview");

    // Restore back to original
    fireEvent.change(screen.getByLabelText(/rule 0 pattern/i), { target: { value: "original_pattern" } });

    // Resolve preview
    resolvePreview!(json({
      rule: 0,
      resource: "entities",
      source: "location",
      row_count: 2,
      matched_count: 2,
      rows: [
        { id: { resource: "entities", key: "1" }, source_value: "val", matched: true, value_truncated: false },
      ],
    }));

    // Wait a bit and verify results panel is not displayed and preview button is not stuck
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByText(/Matched 2 of 2 rows/i)).not.toBeInTheDocument();
    expect(previewBtn).toHaveTextContent("Preview");
  });

  it("7.9 The earlier of two overlapping requests for one rule does not overwrite the later response when it arrives last", async () => {
    let resolveReq1: ((res: Response) => void) | undefined;
    let resolveReq2: ((res: Response) => void) | undefined;
    let callCount = 0;

    fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/connections") {
        return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb.lan", enabled: true, has_credential: true, transforms: [{ resource: "entities", source: "location", pattern: "pattern" }] }]);
      }
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      if (url === "/api/connections/c1/schema") return json(defaultSchema);
      if (url === "/api/connections/c1/transforms/preview") {
        callCount += 1;
        if (callCount === 1) {
          return new Promise<Response>((r) => { resolveReq1 = r; });
        } else {
          return new Promise<Response>((r) => { resolveReq2 = r; });
        }
      }
      throw new Error(`unexpected fetch: ${url}`);
    }) as ReturnType<typeof stubFetch>;
    vi.stubGlobal("fetch", fetchMock);

    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /^edit$/i }));
    const previewBtn = await screen.findByRole("button", { name: /preview rule 0/i });

    // Issue first preview
    fireEvent.click(previewBtn);
    // Issue second preview
    fireEvent.click(previewBtn);

    // Resolve second request first
    resolveReq2!(json({
      rule: 0,
      resource: "entities",
      source: "location",
      row_count: 5,
      matched_count: 5,
      rows: [
        { id: { resource: "entities", key: "1" }, source_value: "val2", matched: true, value_truncated: false },
      ],
    }));

    expect(await screen.findByText(/Matched 5 of 5 rows/i)).toBeInTheDocument();

    // Resolve first request later
    resolveReq1!(json({
      rule: 0,
      resource: "entities",
      source: "location",
      row_count: 1,
      matched_count: 1,
      rows: [
        { id: { resource: "entities", key: "1" }, source_value: "val1", matched: true, value_truncated: false },
      ],
    }));

    // Wait a bit and verify later response was not overwritten
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.getByText(/Matched 5 of 5 rows/i)).toBeInTheDocument();
    expect(screen.queryByText(/Matched 1 of 1 rows/i)).not.toBeInTheDocument();
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

  it("an editor opened on the deleted connection after delete started closes when delete succeeds with refetch failed", async () => {
    let schemaRequests = 0;
    let deleteStarted = false;
    let resolveDelete!: (res: Response) => void;
    const deletePromise = new Promise<Response>((res) => { resolveDelete = res; });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = (init?.method ?? "GET").toUpperCase();
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      if (url === "/api/connections" && method === "GET") {
        if (deleteStarted) return json({ error: "Failed" }, 500);
        return json([
          { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true, transforms: [] },
          { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", enabled: true, has_credential: true, transforms: [] },
        ]);
      }
      if (url.includes("/api/connections/c1/schema") && method === "GET") {
        schemaRequests++;
        return json(defaultSchema);
      }
      if (url.startsWith("/api/connections/c1") && method === "DELETE") {
        deleteStarted = true;
        return deletePromise;
      }
      throw new Error(`unexpected fetch: ${url} ${method}`);
    }) as ReturnType<typeof stubFetch>;
    vi.stubGlobal("fetch", fetchMock);

    renderSection(qc);
    await screen.findByText("Home 1");

    // 1. Confirm deleting c1 with no editor open
    const rows = screen.getAllByRole("row");
    const c1Row = rows[1];
    fireEvent.click(within(c1Row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(screen.getByRole("button", { name: /^confirm$/i }));

    await waitFor(() => expect(deleteStarted).toBe(true));

    // 2. Open editor for c1 while delete is in-flight
    fireEvent.click(within(c1Row).getByRole("button", { name: /^edit$/i }));
    await screen.findByDisplayValue("Home 1");
    expect(schemaRequests).toBe(1);

    // 3. Complete the delete
    await act(async () => {
      resolveDelete(new Response(null, { status: 204 }));
    });

    // 4. Editor must close
    await waitFor(() => {
      expect(screen.queryByDisplayValue("Home 1")).not.toBeInTheDocument();
    });

    // 5. Deleted connection's schema is not held and not requested again
    expect(qc.getQueryData(["connector-schema", "c1"])).toBeUndefined();
    expect(schemaRequests).toBe(1);
  });

  it("when collapsed and reopened before delete completes, editor on deleted connection closes", async () => {
    let schemaRequests = 0;
    let deleteStarted = false;
    let resolveDelete!: (res: Response) => void;
    const deletePromise = new Promise<Response>((res) => { resolveDelete = res; });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = (init?.method ?? "GET").toUpperCase();
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      if (url === "/api/connections" && method === "GET") {
        if (deleteStarted) return json({ error: "Failed" }, 500);
        return json([
          { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true, transforms: [] },
          { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", enabled: true, has_credential: true, transforms: [] },
        ]);
      }
      if (url.includes("/api/connections/c1/schema") && method === "GET") {
        schemaRequests++;
        return json(defaultSchema);
      }
      if (url.startsWith("/api/connections/c1") && method === "DELETE") {
        deleteStarted = true;
        return deletePromise;
      }
      throw new Error(`unexpected fetch: ${url} ${method}`);
    }) as ReturnType<typeof stubFetch>;
    vi.stubGlobal("fetch", fetchMock);

    function Collapsible() {
      const [show, setShow] = useState(true);
      return (
        <QueryClientProvider client={qc}>
          <ToastProvider>
            <button type="button" onClick={() => setShow((s) => !s)}>Toggle Block</button>
            {show && <ConnectionsSection />}
          </ToastProvider>
        </QueryClientProvider>
      );
    }
    render(<Collapsible />);
    await screen.findByText("Home 1");

    // 1. Confirm deleting c1
    const rows = screen.getAllByRole("row");
    const c1Row = rows[1];
    fireEvent.click(within(c1Row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(screen.getByRole("button", { name: /^confirm$/i }));
    await waitFor(() => expect(deleteStarted).toBe(true));

    // 2. Collapse and reopen block
    fireEvent.click(screen.getByRole("button", { name: "Toggle Block" }));
    expect(screen.queryByText("Home 1")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Toggle Block" }));
    await screen.findByText("Home 1");

    // 3. Open editor for c1 before delete completes
    const newRows = screen.getAllByRole("row");
    fireEvent.click(within(newRows[1]).getByRole("button", { name: /^edit$/i }));
    await screen.findByDisplayValue("Home 1");

    // 4. Resolve delete
    await act(async () => {
      resolveDelete(new Response(null, { status: 204 }));
    });

    // 5. Editor for c1 closes
    await waitFor(() => {
      expect(screen.queryByDisplayValue("Home 1")).not.toBeInTheDocument();
    });
    expect(qc.getQueryData(["connector-schema", "c1"])).toBeUndefined();
    expect(schemaRequests).toBe(1);
  });

  it("when collapsed and reopened before delete completes, editor open on another connection stays open", async () => {
    let deleteStarted = false;
    let resolveDelete!: (res: Response) => void;
    const deletePromise = new Promise<Response>((res) => { resolveDelete = res; });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = (init?.method ?? "GET").toUpperCase();
      if (url === "/api/settings") return json({ default_connection_id: { value: null, is_default: true } });
      if (url === "/api/connections" && method === "GET") {
        if (deleteStarted) return json({ error: "Failed" }, 500);
        return json([
          { id: "c1", connector: "homebox", name: "Home 1", base_url: "http://hb1", enabled: true, has_credential: true, transforms: [] },
          { id: "c2", connector: "homebox", name: "Home 2", base_url: "http://hb2", enabled: true, has_credential: true, transforms: [] },
        ]);
      }
      if (url.includes("/api/connections/c2/schema") && method === "GET") {
        return json(defaultSchema);
      }
      if (url.startsWith("/api/connections/c1") && method === "DELETE") {
        deleteStarted = true;
        return deletePromise;
      }
      throw new Error(`unexpected fetch: ${url} ${method}`);
    }) as ReturnType<typeof stubFetch>;
    vi.stubGlobal("fetch", fetchMock);

    function Collapsible() {
      const [show, setShow] = useState(true);
      return (
        <QueryClientProvider client={qc}>
          <ToastProvider>
            <button type="button" onClick={() => setShow((s) => !s)}>Toggle Block</button>
            {show && <ConnectionsSection />}
          </ToastProvider>
        </QueryClientProvider>
      );
    }
    render(<Collapsible />);
    await screen.findByText("Home 1");

    // 1. Confirm deleting c1
    const rows = screen.getAllByRole("row");
    const c1Row = rows[1];
    fireEvent.click(within(c1Row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(screen.getByRole("button", { name: /^confirm$/i }));
    await waitFor(() => expect(deleteStarted).toBe(true));

    // 2. Collapse and reopen block
    fireEvent.click(screen.getByRole("button", { name: "Toggle Block" }));
    expect(screen.queryByText("Home 1")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Toggle Block" }));
    await screen.findByText("Home 1");

    // 3. Open editor for c2 (another connection) before delete completes
    const newRows = screen.getAllByRole("row");
    fireEvent.click(within(newRows[2]).getByRole("button", { name: /^edit$/i }));
    await screen.findByDisplayValue("Home 2");

    // 4. Resolve delete of c1
    await act(async () => {
      resolveDelete(new Response(null, { status: 204 }));
    });

    // 5. Editor for c2 stays open!
    expect(screen.getByDisplayValue("Home 2")).toBeInTheDocument();
  });
});
