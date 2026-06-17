# Homebox Integration Plan B (Frontend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the UI for using a Homebox inventory as a label data source: a Connections management screen, a generic schema-driven browse/drill-down UI for any connector, field mapping into the existing LabelGrid, and a materialize → batch → PDF/print flow.

**Architecture:** A new `src/api/connectors.ts` layer (types + react-query hooks + imperative browse/materialize calls) talks to the Plan A backend. A `ConnectionsSection` (mirroring `PrintersSection`) does CRUD in Settings. A new `Connect` page composes a generic `ConnectorBrowser` (renders any `schema()`: table/tree, filters, cursor pagination, direct drill-down, multi-select) with a field-mapping panel and the existing `LabelGrid` + `/api/batch` flow. A pure `connectorRows.ts` lib turns materialized rows + a field mapping into `LabelGridRow[]` (`origin: "connector"`, which the row model already supports).

**Tech Stack:** React 19, TypeScript, Vite, `@tanstack/react-query` 5, `react-router-dom` 7, `react-data-grid` 7, Vitest 4 + `@testing-library/react`. Tailwind + CSS variables for styling.

## Global Constraints
- All work is under `ui/`. Commands run from `ui/`: `npm run test` (vitest), `npm run lint` (eslint), `npm run build` (`tsc -b && vite build`). All three must be clean before each commit.
- No em dashes in code or copy. New files end with a newline. TypeScript strict; no `any` (use `unknown` + narrowing, as the codebase does).
- Match existing conventions: styling via Tailwind classes + inline `style={{ ... }}` CSS variables (`var(--surface)`, `var(--border)`, `var(--ink)`, `var(--muted)`, `var(--accent)`, `var(--bad)`, `var(--accent-ink, #fff)`); the shared class consts `inputClass`/`buttonBase` seen in `PrintersSection.tsx`/`Import.tsx`.
- API access goes through `src/api/client.ts` helpers (`getJson`, `sendJson`, `del`, `submitBatch`, `saveBlob`) and react-query hooks in the api layer; components never call `fetch` directly (except the established blob/source cases).
- Tests use the established pattern: a stateful `stubFetch()` installed with `vi.stubGlobal("fetch", ...)`, components rendered inside `QueryClientProvider` + `ToastProvider`, queries created with `retry: false`. See `src/pages/settings/PrintersSection.test.tsx`.
- Backend is Plan A (already merged). Endpoints + exact JSON shapes are in the "Backend contract" section below; do not invent fields.
- Spec: `docs/superpowers/specs/2026-06-16-homebox-integration-design.md` and the framework spec `docs/superpowers/specs/2026-06-16-api-integration-framework-design.md`.

---

## Backend contract (verified from Plan A)
- `GET /api/connections` -> `Connection[]` where `Connection = { id, connector, name, base_url, enabled, has_credential }` (NO `credential` ever).
- `POST /api/connections` body `{ connector, name, base_url, credential, enabled? }` -> 201 `Connection`.
- `GET /api/connections/{id}` -> `Connection`. `PUT /api/connections/{id}` body `{ connector, name, base_url, credential?, enabled }` -> `Connection` (omitting/empty `credential` keeps the stored one). `DELETE /api/connections/{id}` -> 204.
- `GET /api/connections/{id}/schema` -> `ConnectorSchema = { version, resources: ResourceSpec[], relationships: RelationshipSpec[] }`. `ResourceSpec = { id, label, view: "table"|"tree", columns: FieldSpec[], filters: FilterSpec[] }`. `FieldSpec = { key, label, ty, tier }` with `ty in {text,number,money,date,badge}`, `tier in {cheap,hydrated,derived}`. `FilterSpec = { key, label, ty }` with `ty in {search,location_id,label_id}`. `RelationshipSpec = { id, label, from, to }`.
- `POST /api/connections/{id}/browse` body `{ resource, filters?: Record<string,string>, parent?: { relationship, key }, cursor?: string, page_size?: number }` -> `BrowsePage = { rows: DisplayRow[], next_cursor: string|null, has_more: boolean, count: number|null }`. `DisplayRow = { id: { resource, key }, cells: Record<string, string|number> }`. A bad/foreign cursor returns HTTP 400.
- `POST /api/connections/{id}/materialize` body `{ rows: { resource, key }[], fields: string[], expansion: "as_listed" }` -> `LabelRowResult[]` where `LabelRowResult = { source: { resource, key }, data: Record<string,string> }`. Capped at 200 rows (400 `BudgetExceeded` above that).
- Homebox specifics the UI relies on: the `entities` resource (`view: "table"`) carries `entityType` as a column (items + locations interleaved); the `locations` resource (`view: "tree"`) is a FLAT list (the backend flattens the tree, depth is not preserved in v1); the relationship `location_children` (`from: "locations"`, `to: "entities"`) drives drill-down (browse `entities` with `parent: { relationship: "location_children", key: <locationKey> }`). Derived URL fields (e.g. `item_url`) are `tier: "derived"` columns resolved at materialize.

## File structure
- Create `src/api/connectors.ts` — connector types + hooks (`useConnections`, `useSaveConnection`, `useDeleteConnection`, `useConnectorSchema`) + imperative `browseConnection`/`materializeConnection`.
- Create `src/pages/settings/ConnectionsSection.tsx` (+ test) — Connections CRUD; wired into `src/pages/Settings.tsx`.
- Create `src/lib/connectorRows.ts` (+ test) — pure: field mapping + materialized rows -> `LabelGridRow[]`.
- Create `src/pages/connect/ConnectorBrowser.tsx` (+ test) — generic schema-driven browse (table/tree, filters, pagination, drill-down, multi-select).
- Create `src/pages/Connect.tsx` (+ test) — the page: connection picker -> browser -> field mapping -> template/options/copies/printer -> LabelGrid -> download/print. Route in `src/app/App.tsx`; nav item in `src/app/Shell.tsx`.
- Modify `docs/SPEC.md` (UI note) in the final task.

---

### Task 1: Connector API layer (`src/api/connectors.ts`)

**Files:** Create `src/api/connectors.ts`; Test: `src/api/connectors.test.ts`.

**Interfaces:**
- Produces (types): `Connection`, `ConnectionInput`, `ConnectorView`, `FieldType`, `FilterType`, `Tier`, `FieldSpec`, `FilterSpec`, `ResourceSpec`, `RelationshipSpec`, `ConnectorSchema`, `RowRef`, `CellValue`, `DisplayRow`, `BrowseParent`, `BrowseRequest`, `BrowsePage`, `MaterializeRequest`, `LabelRowResult`.
- Produces (functions): `useConnections()`, `useSaveConnection()`, `useDeleteConnection()`, `useConnectorSchema(id: string)`, `browseConnection(id: string, req: BrowseRequest): Promise<BrowsePage>`, `materializeConnection(id: string, req: MaterializeRequest): Promise<LabelRowResult[]>`.

- [ ] **Step 1: Write the failing test** (`src/api/connectors.test.ts`):
```ts
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { browseConnection, materializeConnection } from "./connectors";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

describe("connectors api", () => {
  beforeEach(() => vi.unstubAllGlobals());
  afterEach(() => vi.unstubAllGlobals());

  it("browseConnection posts the request and returns the page", async () => {
    const fetchMock = vi.fn(async () =>
      json({ rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill" } }], next_cursor: null, has_more: false, count: 1 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const page = await browseConnection("c1", { resource: "entities" });
    expect(page.rows[0].id.key).toBe("e1");
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toBe("/api/connections/c1/browse");
    expect((init as RequestInit).method).toBe("POST");
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({ resource: "entities" });
  });

  it("materializeConnection returns label rows", async () => {
    const fetchMock = vi.fn(async () => json([{ source: { resource: "entities", key: "e1" }, data: { name: "Drill" } }]));
    vi.stubGlobal("fetch", fetchMock);
    const rows = await materializeConnection("c1", { rows: [{ resource: "entities", key: "e1" }], fields: ["name"], expansion: "as_listed" });
    expect(rows[0].data.name).toBe("Drill");
  });
});
```

- [ ] **Step 2: Run it, watch it fail** — `npm run test -- connectors` (FAIL: module missing).

- [ ] **Step 3: Implement `src/api/connectors.ts`:**
```ts
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getJson, sendJson, del } from "./client";

export interface Connection {
  id: string;
  connector: string;
  name: string;
  base_url: string;
  enabled: boolean;
  has_credential: boolean;
}
export interface ConnectionInput {
  connector: string;
  name: string;
  base_url: string;
  credential?: string;
  enabled?: boolean;
}

export type ConnectorView = "table" | "tree";
export type FieldType = "text" | "number" | "money" | "date" | "badge";
export type FilterType = "search" | "location_id" | "label_id";
export type Tier = "cheap" | "hydrated" | "derived";

export interface FieldSpec { key: string; label: string; ty: FieldType; tier: Tier }
export interface FilterSpec { key: string; label: string; ty: FilterType }
export interface ResourceSpec { id: string; label: string; view: ConnectorView; columns: FieldSpec[]; filters: FilterSpec[] }
export interface RelationshipSpec { id: string; label: string; from: string; to: string }
export interface ConnectorSchema { version: string; resources: ResourceSpec[]; relationships: RelationshipSpec[] }

export interface RowRef { resource: string; key: string }
export type CellValue = string | number; // backend untagged Text|Number
export interface DisplayRow { id: RowRef; cells: Record<string, CellValue> }
export interface BrowseParent { relationship: string; key: string }
export interface BrowseRequest {
  resource: string;
  filters?: Record<string, string>;
  parent?: BrowseParent;
  cursor?: string;
  page_size?: number;
}
export interface BrowsePage { rows: DisplayRow[]; next_cursor: string | null; has_more: boolean; count: number | null }

export interface MaterializeRequest { rows: RowRef[]; fields: string[]; expansion: "as_listed" }
export interface LabelRowResult { source: RowRef; data: Record<string, string> }

export function useConnections() {
  return useQuery({ queryKey: ["connections"], queryFn: () => getJson<Connection[]>("/connections") });
}

export function useSaveConnection() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ input, id }: { input: ConnectionInput; id?: string }) =>
      id === undefined
        ? sendJson<Connection>("POST", "/connections", input)
        : sendJson<Connection>("PUT", `/connections/${encodeURIComponent(id)}`, input),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["connections"] }),
  });
}

export function useDeleteConnection() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => del(`/connections/${encodeURIComponent(id)}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["connections"] }),
  });
}

export function useConnectorSchema(id: string) {
  return useQuery({
    queryKey: ["connector-schema", id],
    queryFn: () => getJson<ConnectorSchema>(`/connections/${encodeURIComponent(id)}/schema`),
    enabled: !!id,
  });
}

export function browseConnection(id: string, req: BrowseRequest): Promise<BrowsePage> {
  return sendJson<BrowsePage>("POST", `/connections/${encodeURIComponent(id)}/browse`, req);
}

export function materializeConnection(id: string, req: MaterializeRequest): Promise<LabelRowResult[]> {
  return sendJson<LabelRowResult[]>("POST", `/connections/${encodeURIComponent(id)}/materialize`, req);
}
```

- [ ] **Step 4: Run tests** — `npm run test -- connectors` (2 pass). `npm run lint` clean.

- [ ] **Step 5: Commit**
```bash
git add ui/src/api/connectors.ts ui/src/api/connectors.test.ts
git commit -m "feat(ui): connector API layer (types + hooks + browse/materialize)"
```

---

### Task 2: Connections settings section (`src/pages/settings/ConnectionsSection.tsx`)

**Files:** Create `src/pages/settings/ConnectionsSection.tsx` (+ `.test.tsx`); Modify `src/pages/Settings.tsx` (render the section).

**Interfaces:**
- Consumes: `useConnections`, `useSaveConnection`, `useDeleteConnection`, `Connection`, `ConnectionInput` from Task 1; `useToast` from `../../app/toast-context`.
- Produces: `export function ConnectionsSection()`.

- [ ] **Step 1: Write the failing test** (`ConnectionsSection.test.tsx`) using the stateful-stub pattern from `PrintersSection.test.tsx`:
```tsx
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { ConnectionsSection } from "./ConnectionsSection";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

type C = { id: string; connector: string; name: string; base_url: string; enabled: boolean; has_credential: boolean };

function stubFetch() {
  let state: C[] = [];
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.startsWith("/api/connections/") && method === "DELETE") {
      const id = decodeURIComponent(url.slice("/api/connections/".length));
      state = state.filter((c) => c.id !== id);
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/connections") && method === "POST") {
      const b = JSON.parse(init!.body as string) as ConnectionInputBody;
      const c: C = { id: "id1", connector: b.connector, name: b.name, base_url: b.base_url, enabled: true, has_credential: !!b.credential };
      state = [...state, c];
      return json(c, 201);
    }
    if (url.startsWith("/api/connections")) return json(state);
    throw new Error(`unexpected fetch: ${url}`);
  });
}
type ConnectionInputBody = { connector: string; name: string; base_url: string; credential?: string };

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
  beforeEach(() => { vi.unstubAllGlobals(); fetchMock = stubFetch(); vi.stubGlobal("fetch", fetchMock); });
  afterEach(() => vi.unstubAllGlobals());

  it("creates a connection and never displays the credential", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/name/i), { target: { value: "Home" } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "hb_secret" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText("Home")).toBeInTheDocument();
    // the secret value is never rendered anywhere
    expect(screen.queryByText("hb_secret")).not.toBeInTheDocument();
    // POST carried the credential
    const post = fetchMock.mock.calls.find(([u, i]) => String(u) === "/api/connections" && (i as RequestInit)?.method === "POST");
    expect(JSON.parse((post![1] as RequestInit).body as string).credential).toBe("hb_secret");
  });

  it("requires an api key when creating", async () => {
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: /add connection/i }));
    fireEvent.change(screen.getByLabelText(/name/i), { target: { value: "Home" } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: "http://hb.lan:7745" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText(/api key is required/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run it, watch it fail** — `npm run test -- ConnectionsSection` (FAIL).

- [ ] **Step 3: Implement `ConnectionsSection.tsx`** (model it on `PrintersSection.tsx`; the form has connector [fixed "homebox" for now], name, base_url, api key, enabled; editing an existing connection leaves the key blank with a "leave blank to keep" hint and omits `credential` when blank so the backend keeps it; the table shows name/base_url/enabled and `has_credential`):
```tsx
import { useState } from "react";
import { useConnections, useSaveConnection, useDeleteConnection, type Connection, type ConnectionInput } from "../../api/connectors";
import { useToast } from "../../app/toast-context";

const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

function ConnectionForm({ initial, onClose }: { initial: Connection | null; onClose: () => void }) {
  const isNew = initial === null;
  const [name, setName] = useState(initial?.name ?? "");
  const [baseUrl, setBaseUrl] = useState(initial?.base_url ?? "");
  const [apiKey, setApiKey] = useState("");
  const [enabled, setEnabled] = useState(initial?.enabled ?? true);
  const [error, setError] = useState<string | null>(null);
  const save = useSaveConnection();
  const { push } = useToast();

  const submit = () => {
    if (name.trim() === "") { setError("name must not be empty"); return; }
    let url: URL;
    try { url = new URL(baseUrl.trim()); } catch { setError("base url must be a valid URL"); return; }
    if (url.protocol !== "http:" && url.protocol !== "https:") { setError("base url must be http or https"); return; }
    if (isNew && apiKey.trim() === "") { setError("api key is required"); return; }
    setError(null);
    const input: ConnectionInput = {
      connector: initial?.connector ?? "homebox",
      name: name.trim(),
      base_url: baseUrl.trim(),
      enabled,
      // Send the key only when provided; on edit, a blank key means "keep the stored one".
      ...(apiKey.trim() !== "" ? { credential: apiKey.trim() } : {}),
    };
    save.mutate(
      { input, id: initial?.id },
      {
        onSuccess: () => { push({ kind: "ok", message: `Saved ${input.name}` }); onClose(); },
        onError: (err) => {
          const message = err instanceof Error ? err.message : "Save failed";
          setError(message); push({ kind: "error", message });
        },
      },
    );
  };

  return (
    <div className="flex flex-col gap-3 rounded-md border p-4" style={{ borderColor: "var(--border)" }}>
      <div className="flex flex-wrap gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>connector</span>
          {/* Only Homebox exists today; shown read-only so the user knows what they are configuring. */}
          <select aria-label="connector" value={initial?.connector ?? "homebox"} disabled className={inputClass} style={inputStyle}>
            <option value="homebox">homebox</option>
          </select>
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>name</span>
          <input aria-label="name" value={name} onChange={(e) => setName(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>base url</span>
          <input aria-label="base url" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="http://homebox.lan:7745" className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>api key{isNew ? "" : " (leave blank to keep)"}</span>
          <input aria-label="api key" type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex items-center gap-2 self-end pb-2">
          <input type="checkbox" aria-label="enabled" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
          <span className="text-sm">enabled</span>
        </label>
      </div>
      {error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
      <div className="flex gap-3">
        <button type="button" onClick={submit} disabled={save.isPending} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}>Save</button>
        <button type="button" onClick={onClose} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>Cancel</button>
      </div>
    </div>
  );
}

function ConnectionRow({ conn, onEdit, onDeleted }: { conn: Connection; onEdit: () => void; onDeleted: (id: string) => void }) {
  const [confirming, setConfirming] = useState(false);
  const remove = useDeleteConnection();
  const { push } = useToast();
  const td = "px-3 py-2 text-sm";
  return (
    <tr style={{ borderTop: "1px solid var(--border)" }}>
      <td className={td}>{conn.name}</td>
      <td className={`${td} font-mono`}>{conn.connector}</td>
      <td className={`${td} font-mono`}>{conn.base_url}</td>
      <td className={td}>{conn.has_credential ? "set" : "none"}</td>
      <td className={td}>{conn.enabled ? "yes" : "no"}</td>
      <td className={`${td} flex gap-2`}>
        <button type="button" onClick={onEdit} className="underline" style={{ color: "var(--ink)" }}>Edit</button>
        {confirming ? (
          <>
            <button type="button" disabled={remove.isPending} onClick={() =>
              remove.mutate(conn.id, {
                onSuccess: () => { push({ kind: "ok", message: `Deleted ${conn.name}` }); onDeleted(conn.id); },
                onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Delete failed" }),
              })
            } style={{ color: "var(--bad)" }}>Confirm</button>
            <button type="button" onClick={() => setConfirming(false)} style={{ color: "var(--muted)" }}>Cancel</button>
          </>
        ) : (
          <button type="button" onClick={() => setConfirming(true)} style={{ color: "var(--bad)" }}>Delete</button>
        )}
      </td>
    </tr>
  );
}

export function ConnectionsSection() {
  const { data: connections, isPending, isError } = useConnections();
  const [editing, setEditing] = useState<Connection | "new" | null>(null);
  const th = "px-3 py-2 text-left text-xs font-medium";
  const onDeleted = (id: string) => { if (editing !== null && editing !== "new" && editing.id === id) setEditing(null); };
  return (
    <section className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Connections</h2>
        <button type="button" onClick={() => setEditing("new")} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>Add connection</button>
      </div>
      {editing !== null && (
        <ConnectionForm key={editing === "new" ? "new" : editing.id} initial={editing === "new" ? null : editing} onClose={() => setEditing(null)} />
      )}
      {isPending ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>Loading connections...</p>
      ) : isError ? (
        <p className="text-sm" style={{ color: "var(--bad)" }}>Failed to load connections.</p>
      ) : (connections ?? []).length === 0 ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>No connections configured.</p>
      ) : (
        <table className="w-full border-collapse">
          <thead>
            <tr>
              <th className={th} style={{ color: "var(--muted)" }}>Name</th>
              <th className={th} style={{ color: "var(--muted)" }}>Connector</th>
              <th className={th} style={{ color: "var(--muted)" }}>Base URL</th>
              <th className={th} style={{ color: "var(--muted)" }}>API key</th>
              <th className={th} style={{ color: "var(--muted)" }}>Enabled</th>
              <th className={th} style={{ color: "var(--muted)" }}></th>
            </tr>
          </thead>
          <tbody>
            {(connections ?? []).map((c) => (
              <ConnectionRow key={c.id} conn={c} onEdit={() => setEditing(c)} onDeleted={onDeleted} />
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
```

- [ ] **Step 4: Wire into Settings** — in `src/pages/Settings.tsx`, import `ConnectionsSection` and render `<ConnectionsSection />` alongside the existing sections (follow the existing composition; place it after `PrintersSection`). Run `npm run test -- Settings` to confirm the Settings page test still passes (if it asserts a fixed set of sections, update it to include Connections).

- [ ] **Step 5: Run** — `npm run test -- ConnectionsSection` (pass), `npm run lint`, `npm run build`.

- [ ] **Step 6: Commit**
```bash
git add ui/src/pages/settings/ConnectionsSection.tsx ui/src/pages/settings/ConnectionsSection.test.tsx ui/src/pages/Settings.tsx
git commit -m "feat(ui): connections management section in settings"
```

---

### Task 3: Materialized rows -> LabelGridRow mapping (`src/lib/connectorRows.ts`)

**Files:** Create `src/lib/connectorRows.ts` (+ `.test.ts`).

**Interfaces:**
- Consumes: `LabelGridRow`, `newId` from `./labelGrid`; `LabelRowResult` from `../api/connectors`.
- Produces: `type FieldMapping = Record<string, string>`; `defaultMapping(templateFields, connectorFieldKeys): FieldMapping`; `mappedConnectorKeys(mapping): string[]`; `rowsFromMaterialized(results, mapping, connector, connection): LabelGridRow[]`.

- [ ] **Step 1: Write the failing test** (`connectorRows.test.ts`):
```ts
import { describe, it, expect } from "vitest";
import { defaultMapping, mappedConnectorKeys, rowsFromMaterialized } from "./connectorRows";

describe("connectorRows", () => {
  it("defaultMapping matches template fields to identically-named connector keys", () => {
    const m = defaultMapping(["name", "sku", "qty"], ["name", "qty", "manufacturer"]);
    expect(m).toEqual({ name: "name", sku: "", qty: "qty" });
  });

  it("mappedConnectorKeys returns distinct non-empty targets", () => {
    expect(mappedConnectorKeys({ a: "name", b: "name", c: "" }).sort()).toEqual(["name"]);
  });

  it("rowsFromMaterialized builds connector-origin rows with mapped data and source", () => {
    const rows = rowsFromMaterialized(
      [{ source: { resource: "entities", key: "e1" }, data: { name: "Drill", manufacturer: "Acme" } }],
      { title: "name", maker: "manufacturer", blank: "" },
      "homebox",
      "c1",
    );
    expect(rows).toHaveLength(1);
    expect(rows[0].origin).toBe("connector");
    expect(rows[0].source).toEqual({ connector: "homebox", connection: "c1", resource: "entities", key: "e1" });
    expect(rows[0].data).toEqual({ title: "Drill", maker: "Acme", blank: "" });
    expect(rows[0].option).toEqual({});
  });
});
```

- [ ] **Step 2: Run it, watch it fail** — `npm run test -- connectorRows` (FAIL).

- [ ] **Step 3: Implement `src/lib/connectorRows.ts`:**
```ts
import { newId, type LabelGridRow } from "./labelGrid";
import type { LabelRowResult } from "../api/connectors";

// Maps a template field name -> a connector field key (or "" to leave the field blank).
export type FieldMapping = Record<string, string>;

// Pre-fill the mapping: a template field is mapped to a connector column of the same key when one exists.
export function defaultMapping(templateFields: string[], connectorFieldKeys: string[]): FieldMapping {
  const available = new Set(connectorFieldKeys);
  const mapping: FieldMapping = {};
  for (const field of templateFields) mapping[field] = available.has(field) ? field : "";
  return mapping;
}

// The distinct connector field keys to request from /materialize (drops unmapped fields).
export function mappedConnectorKeys(mapping: FieldMapping): string[] {
  return [...new Set(Object.values(mapping).filter((key) => key !== ""))];
}

// Turn materialized rows into editable grid rows, applying the field mapping. Each row keeps its
// connector source so a later batch can trace back to the Homebox entity.
export function rowsFromMaterialized(
  results: LabelRowResult[],
  mapping: FieldMapping,
  connector: string,
  connection: string,
): LabelGridRow[] {
  return results.map((result) => {
    const data: Record<string, string> = {};
    for (const [field, key] of Object.entries(mapping)) {
      data[field] = key ? (result.data[key] ?? "") : "";
    }
    return {
      id: newId(),
      origin: "connector",
      source: { connector, connection, resource: result.source.resource, key: result.source.key },
      data,
      option: {},
      validation: {},
    };
  });
}
```

- [ ] **Step 4: Run tests** — `npm run test -- connectorRows` (3 pass). `npm run lint`.

- [ ] **Step 5: Commit**
```bash
git add ui/src/lib/connectorRows.ts ui/src/lib/connectorRows.test.ts
git commit -m "feat(ui): pure mapping from materialized connector rows to label grid rows"
```

---

### Task 4: Generic connector browser (`src/pages/connect/ConnectorBrowser.tsx`)

**Files:** Create `src/pages/connect/ConnectorBrowser.tsx` (+ `.test.tsx`).

**Interfaces:**
- Consumes: `ConnectorSchema`, `ResourceSpec`, `DisplayRow`, `RowRef`, `BrowseRequest`, `browseConnection` from `../../api/connectors`.
- Produces: `export function ConnectorBrowser({ connectionId, schema, selected, onSelectedChange }: ConnectorBrowserProps)` where
```ts
export interface ConnectorBrowserProps {
  connectionId: string;
  schema: ConnectorSchema;
  selected: RowRef[];
  onSelectedChange: (refs: RowRef[]) => void;
}
```

Behavior: a resource selector (one button per `schema.resources`), the active resource's filter inputs (search -> text, location_id/label_id -> text id), a table of `DisplayRow`s (a checkbox column + one column per `resource.columns`), a "Load more" button when `has_more` (appends the next cursor page), and on a `tree`/location row a "Drill in" action that switches to the related `to` resource browsing with `parent: { relationship, key }`. Selection is tracked by `"<resource>:<key>"` and round-tripped through `selected`/`onSelectedChange` so the parent owns it.

- [ ] **Step 1: Write the failing test** (`ConnectorBrowser.test.tsx`):
```tsx
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { useState } from "react";
import { ConnectorBrowser } from "./ConnectorBrowser";
import type { ConnectorSchema, RowRef } from "../../api/connectors";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const schema: ConnectorSchema = {
  version: "homebox-1",
  resources: [
    { id: "entities", label: "Items", view: "table",
      columns: [{ key: "name", label: "Name", ty: "text", tier: "cheap" }, { key: "entityType", label: "Type", ty: "badge", tier: "cheap" }],
      filters: [{ key: "q", label: "Search", ty: "search" }] },
  ],
  relationships: [],
};

function Harness() {
  const [selected, setSelected] = useState<RowRef[]>([]);
  return (
    <div>
      <span data-testid="count">{selected.length}</span>
      <ConnectorBrowser connectionId="c1" schema={schema} selected={selected} onSelectedChange={setSelected} />
    </div>
  );
}

describe("ConnectorBrowser", () => {
  beforeEach(() => vi.unstubAllGlobals());
  afterEach(() => vi.unstubAllGlobals());

  it("loads rows and toggles selection", async () => {
    vi.stubGlobal("fetch", vi.fn(async () =>
      json({ rows: [
        { id: { resource: "entities", key: "e1" }, cells: { name: "Drill", entityType: "item" } },
        { id: { resource: "entities", key: "e2" }, cells: { name: "Shelf", entityType: "location" } },
      ], next_cursor: null, has_more: false, count: 2 })));
    render(<Harness />);
    expect(await screen.findByText("Drill")).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("select entities:e1"));
    expect(screen.getByTestId("count").textContent).toBe("1");
  });

  it("sends the search filter on Apply", async () => {
    const fetchMock = vi.fn(async () => json({ rows: [], next_cursor: null, has_more: false, count: 0 }));
    vi.stubGlobal("fetch", fetchMock);
    render(<Harness />);
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    fireEvent.change(screen.getByLabelText("Search"), { target: { value: "drill" } });
    fireEvent.click(screen.getByRole("button", { name: /apply/i }));
    await waitFor(() => {
      const last = fetchMock.mock.calls.at(-1)!;
      expect(JSON.parse((last[1] as RequestInit).body as string).filters).toEqual({ q: "drill" });
    });
  });
});
```

- [ ] **Step 2: Run it, watch it fail** — `npm run test -- ConnectorBrowser` (FAIL).

- [ ] **Step 3: Implement `src/pages/connect/ConnectorBrowser.tsx`:**
```tsx
import { useEffect, useMemo, useState } from "react";
import {
  browseConnection,
  type ConnectorSchema,
  type DisplayRow,
  type RelationshipSpec,
  type ResourceSpec,
  type RowRef,
} from "../../api/connectors";

const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";
const inputClass = "rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;

export interface ConnectorBrowserProps {
  connectionId: string;
  schema: ConnectorSchema;
  selected: RowRef[];
  onSelectedChange: (refs: RowRef[]) => void;
}

const refKey = (r: RowRef) => `${r.resource}:${r.key}`;

export function ConnectorBrowser({ connectionId, schema, selected, onSelectedChange }: ConnectorBrowserProps) {
  const [resourceId, setResourceId] = useState(schema.resources[0]?.id ?? "");
  const resource = useMemo<ResourceSpec | undefined>(() => schema.resources.find((r) => r.id === resourceId), [schema, resourceId]);
  // Pending filter inputs vs the applied filters that actually drive a browse.
  const [filterDraft, setFilterDraft] = useState<Record<string, string>>({});
  const [applied, setApplied] = useState<Record<string, string>>({});
  const [parent, setParent] = useState<{ relationship: string; key: string; label: string } | undefined>(undefined);
  const [rows, setRows] = useState<DisplayRow[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectedKeys = useMemo(() => new Set(selected.map(refKey)), [selected]);

  // Fresh page whenever the resource, applied filters, or parent changes. An `active` flag prevents a
  // slow earlier request from overwriting a newer one when the user switches resources / drills quickly
  // (the stale request's setRows is dropped). `resource` is memoized on resourceId so this never loops.
  useEffect(() => {
    if (!resource) return;
    let active = true;
    setBusy(true);
    setError(null);
    (async () => {
      try {
        const page = await browseConnection(connectionId, {
          resource: resource.id,
          ...(Object.keys(applied).length ? { filters: applied } : {}),
          ...(parent ? { parent: { relationship: parent.relationship, key: parent.key } } : {}),
        });
        if (!active) return;
        setRows(page.rows);
        setCursor(page.next_cursor);
        setHasMore(page.has_more);
      } catch (err) {
        if (active) setError(err instanceof Error ? err.message : "Browse failed");
      } finally {
        if (active) setBusy(false);
      }
    })();
    return () => {
      active = false;
    };
  }, [connectionId, resource, applied, parent]);

  // "Load more": append the next cursor page. (Pagination only ever appends to the current resource.)
  const loadMore = async () => {
    if (!resource || !cursor) return;
    setBusy(true);
    setError(null);
    try {
      const page = await browseConnection(connectionId, {
        resource: resource.id,
        ...(Object.keys(applied).length ? { filters: applied } : {}),
        ...(parent ? { parent: { relationship: parent.relationship, key: parent.key } } : {}),
        cursor,
      });
      setRows((prev) => [...prev, ...page.rows]);
      setCursor(page.next_cursor);
      setHasMore(page.has_more);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Browse failed");
    } finally {
      setBusy(false);
    }
  };

  const toggle = (ref: RowRef) => {
    if (selectedKeys.has(refKey(ref))) onSelectedChange(selected.filter((r) => refKey(r) !== refKey(ref)));
    else onSelectedChange([...selected, ref]);
  };

  const relationshipFrom = (rid: string): RelationshipSpec | undefined => schema.relationships.find((rel) => rel.from === rid);
  const drill = (row: DisplayRow, rel: RelationshipSpec) => {
    setParent({ relationship: rel.id, key: row.id.key, label: String(row.cells.name ?? row.id.key) });
    setResourceId(rel.to);
    setApplied({});
    setFilterDraft({});
  };

  const th = "px-3 py-2 text-left text-xs font-medium";
  const td = "px-3 py-2 text-sm";
  const rel = resource ? relationshipFrom(resource.id) : undefined;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        {schema.resources.map((r) => (
          <button
            key={r.id}
            type="button"
            onClick={() => { setResourceId(r.id); setParent(undefined); setApplied({}); setFilterDraft({}); }}
            className={`${buttonBase} border`}
            style={{ borderColor: "var(--border)", color: r.id === resourceId ? "var(--accent)" : "var(--ink)", background: r.id === resourceId ? "var(--accent-soft)" : "transparent" }}
          >
            {r.label}
          </button>
        ))}
        {parent && (
          <span className="text-sm" style={{ color: "var(--muted)" }}>
            in {parent.label}{" "}
            <button type="button" className="underline" onClick={() => setParent(undefined)} style={{ color: "var(--ink)" }}>clear</button>
          </span>
        )}
      </div>

      {resource && resource.filters.length > 0 && (
        <div className="flex flex-wrap items-end gap-2">
          {resource.filters.map((f) => (
            <label key={f.key} className="flex flex-col gap-1">
              <span className="text-xs" style={{ color: "var(--muted)" }}>{f.label}</span>
              <input
                aria-label={f.label}
                value={filterDraft[f.key] ?? ""}
                onChange={(e) => setFilterDraft({ ...filterDraft, [f.key]: e.target.value })}
                className={inputClass}
                style={inputStyle}
              />
            </label>
          ))}
          <button
            type="button"
            onClick={() => setApplied(Object.fromEntries(Object.entries(filterDraft).filter(([, v]) => v.trim() !== "")))}
            className={`${buttonBase} border`}
            style={{ borderColor: "var(--border)", color: "var(--ink)" }}
          >
            Apply
          </button>
        </div>
      )}

      {error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
      {busy && rows.length === 0 && <p className="text-sm" style={{ color: "var(--muted)" }}>Loading...</p>}

      {resource && (
        <table className="w-full border-collapse">
          <thead>
            <tr>
              <th className={th} style={{ color: "var(--muted)" }}></th>
              {resource.columns.map((c) => (
                <th key={c.key} className={th} style={{ color: "var(--muted)" }}>{c.label}</th>
              ))}
              {rel && <th className={th} style={{ color: "var(--muted)" }}></th>}
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={refKey(row.id)} style={{ borderTop: "1px solid var(--border)" }}>
                <td className={td}>
                  <input
                    type="checkbox"
                    aria-label={`select ${refKey(row.id)}`}
                    checked={selectedKeys.has(refKey(row.id))}
                    onChange={() => toggle(row.id)}
                  />
                </td>
                {resource.columns.map((c) => (
                  <td key={c.key} className={td}>{row.cells[c.key] ?? ""}</td>
                ))}
                {rel && (
                  <td className={td}>
                    <button type="button" className="underline" onClick={() => drill(row, rel)} style={{ color: "var(--ink)" }}>Drill in</button>
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <div className="flex items-center gap-3">
        {hasMore && (
          <button type="button" disabled={busy} onClick={() => void loadMore()} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
            Load more
          </button>
        )}
        <span className="text-sm" style={{ color: "var(--muted)" }}>{selected.length} selected</span>
      </div>
    </div>
  );
}
```
Notes: the `tree` view is rendered as the same flat table (the backend already flattened the location tree; depth is not available in v1). Drill-down uses the first relationship whose `from` equals the current resource (`location_children`), browsing the `to` resource (`entities`) with the location as `parent`. The "Apply" button is what commits filters (no per-keystroke browse).

- [ ] **Step 4: Run tests** — `npm run test -- ConnectorBrowser` (2 pass). `npm run lint` clean (the fresh-load effect depends on `[connectionId, resource, applied, parent]`; `resource` is memoized on `resourceId`, so there is no loop and no `exhaustive-deps` suppression is needed).

- [ ] **Step 5: Commit**
```bash
git add ui/src/pages/connect/ConnectorBrowser.tsx ui/src/pages/connect/ConnectorBrowser.test.tsx
git commit -m "feat(ui): generic schema-driven connector browser (filters, pagination, drill-down, selection)"
```

---

### Task 5: Connect page (`src/pages/Connect.tsx`) + route + nav

**Files:** Create `src/pages/Connect.tsx` (+ `.test.tsx`); Modify `src/app/App.tsx` (route), `src/app/Shell.tsx` (nav item).

**Interfaces:**
- Consumes: `useConnections`, `useConnectorSchema`, `materializeConnection`, types from `../api/connectors`; `useTemplates`, `useTemplate`, `usePrinters` from `../api/queries`; `referencedFields`, `defaultOptions` from `../lib/templateFields`; `defaultMapping`, `mappedConnectorKeys`, `rowsFromMaterialized`, `type FieldMapping` from `../lib/connectorRows`; `resolveLabels`, `expandedCount`, `sourceRowForExpandedIndex`, `duplicateRow`, `removeRow`, `MAX_BATCH_LABELS`, `type LabelGridRow` from `../lib/labelGrid`; `LabelGrid` from `../components/LabelGrid`; `submitBatch`, `saveBlob`, `ApiError` from `../api/client`; `useToast` from `../app/toast-context`; `ConnectorBrowser` from `./connect/ConnectorBrowser`.
- Produces: `export function Connect()`.

The page flow, top to bottom: pick a connection; once a schema loads, show the `ConnectorBrowser` and collect selected `RowRef`s; pick a template; show a field-mapping panel (one select per template field, options = the connector's column keys); an "Add N rows" button materializes the selected rows for the mapped connector keys and appends `rowsFromMaterialized(...)` to the grid; then the batch controls (manual options strip, copies, start slot for sheet, printer) + `LabelGrid` + Download/Print, reusing the Import page's run logic (minus CSV option columns: connector rows have no per-row option columns, so validation is just the required-field check).

- [ ] **Step 1: Write the failing test** (`Connect.test.tsx`) covering the end-to-end happy path through mocked fetch (connections, schema, templates, template detail, printers, browse, materialize, batch download):
```tsx
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
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
  layout: { items: [{ type: "text", text: "{name}" }] },
};

function stub() {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url === "/api/connections") return json([{ id: "c1", connector: "homebox", name: "Home", base_url: "http://hb", enabled: true, has_credential: true }]);
    if (url === "/api/connections/c1/schema") return json(schema);
    if (url === "/api/connections/c1/browse") return json({ rows: [{ id: { resource: "entities", key: "e1" }, cells: { name: "Drill" } }], next_cursor: null, has_more: false, count: 1 });
    if (url === "/api/connections/c1/materialize") return json([{ source: { resource: "entities", key: "e1" }, data: { name: "Drill" } }]);
    if (url === "/api/templates") return json({ templates: [{ id: "tpl", name: "Tape", description: "", unit: "mm", dpi: 300, format: { type: "single" } }] });
    if (url === "/api/templates/tpl") return json(templateDetail);
    if (url === "/api/printers") return json([]);
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

describe("Connect", () => {
  beforeEach(() => { vi.unstubAllGlobals(); vi.stubGlobal("fetch", stub()); });
  afterEach(() => vi.unstubAllGlobals());

  it("browses, selects, maps, materializes rows into the grid", async () => {
    renderConnect();
    fireEvent.change(await screen.findByLabelText(/connection/i), { target: { value: "c1" } });
    fireEvent.change(await screen.findByLabelText(/template/i), { target: { value: "tpl" } });
    fireEvent.click(await screen.findByLabelText("select entities:e1"));
    fireEvent.click(await screen.findByRole("button", { name: /add .* row/i }));
    // the materialized value lands in the editable grid
    expect(await screen.findByText("Drill")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run it, watch it fail** — `npm run test -- Connect` (FAIL).

- [ ] **Step 3: Implement `src/pages/Connect.tsx`.** Use this structure (complete code):
```tsx
import { useMemo, useRef, useState } from "react";
import { useConnections, useConnectorSchema, materializeConnection, type ConnectorSchema, type RowRef } from "../api/connectors";
import { ConnectorBrowser } from "./connect/ConnectorBrowser";
import { useTemplates, useTemplate, usePrinters } from "../api/queries";
import { referencedFields, defaultOptions } from "../lib/templateFields";
import { defaultMapping, mappedConnectorKeys, rowsFromMaterialized, type FieldMapping } from "../lib/connectorRows";
import {
  MAX_BATCH_LABELS, expandedCount, resolveLabels, sourceRowForExpandedIndex,
  duplicateRow, removeRow, type LabelGridRow,
} from "../lib/labelGrid";
import { LabelGrid } from "../components/LabelGrid";
import { ApiError, saveBlob, submitBatch } from "../api/client";
import { useToast } from "../app/toast-context";
import type { TemplateDetail } from "../api/types";

type BatchFailures = { failures?: { index: number; code: string; message: string }[] };
const buttonBase = "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";
const inputClass = "rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const MATERIALIZE_CAP = 200; // backend /materialize rejects more than this in one call (400 BudgetExceeded)

export function Connect() {
  const { data: connections } = useConnections();
  const { data: templates } = useTemplates();
  const { data: printers } = usePrinters();

  const [connectionId, setConnectionId] = useState("");
  const { data: schema } = useConnectorSchema(connectionId);
  const [templateId, setTemplateId] = useState("");
  const { data: detail } = useTemplate(templateId);

  const [selected, setSelected] = useState<RowRef[]>([]);
  const conn = (connections ?? []).find((c) => c.id === connectionId);

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">Connect</h1>
      <div className="flex flex-wrap gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">Connection</span>
          <select aria-label="connection" value={connectionId} onChange={(e) => { setConnectionId(e.target.value); setSelected([]); }} className={inputClass} style={inputStyle}>
            <option value="">choose a connection</option>
            {(connections ?? []).filter((c) => c.enabled).map((c) => (<option key={c.id} value={c.id}>{c.name}</option>))}
          </select>
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">Template</span>
          <select aria-label="template" value={templateId} onChange={(e) => setTemplateId(e.target.value)} className={inputClass} style={inputStyle}>
            <option value="">choose a template</option>
            {(templates?.templates ?? []).map((t) => (<option key={t.id} value={t.id}>{t.name}</option>))}
          </select>
        </label>
      </div>

      {connectionId && schema && (
        <ConnectorBrowser key={connectionId} connectionId={connectionId} schema={schema} selected={selected} onSelectedChange={setSelected} />
      )}

      {connectionId && schema && detail && conn && (
        <Composer
          key={`${connectionId}:${detail.id}`}
          connectionId={connectionId}
          connectorId={conn.connector}
          schema={schema}
          detail={detail}
          selected={selected}
          printers={(printers ?? []).filter((p) => p.enabled)}
        />
      )}
    </div>
  );
}

function Composer({
  connectionId, connectorId, schema, detail, selected, printers,
}: {
  connectionId: string;
  connectorId: string;
  schema: ConnectorSchema;
  detail: TemplateDetail;
  selected: RowRef[];
  printers: { id: string; name: string }[];
}) {
  const { push } = useToast();
  // All distinct connector column keys across resources, as mapping targets.
  const connectorKeys = useMemo(() => [...new Set(schema.resources.flatMap((r) => r.columns.map((c) => c.key)))], [schema]);
  const templateFields = useMemo(() => referencedFields(detail.layout, {}), [detail]);
  const [mapping, setMapping] = useState<FieldMapping>(() => defaultMapping(templateFields, connectorKeys));

  const [rows, setRows] = useState<LabelGridRow[]>([]);
  const rowsRef = useRef(rows);
  const commitRows = (next: LabelGridRow[]) => { rowsRef.current = next; setRows(next); };

  const [manualOptions, setManualOptions] = useState<Record<string, string>>(() => defaultOptions(detail.options));
  const [copies, setCopies] = useState(1);
  const [startSlot, setStartSlot] = useState(0);
  const [printer, setPrinter] = useState<string | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const declaredOptions = detail.options ?? {};
  const declaredNames = Object.keys(declaredOptions);
  const isSheet = detail.format.type === "sheet";
  const positions = detail.format.type === "sheet" ? detail.format.positions.length : 0;

  const requiredForRow = (row: LabelGridRow): string[] => referencedFields(detail.layout, { ...manualOptions, ...row.option });
  const requiredUnion = new Set<string>();
  for (const row of rows) for (const f of requiredForRow(row)) requiredUnion.add(f);
  const displayedFields = rows.length ? [...requiredUnion] : referencedFields(detail.layout, manualOptions);

  const validateRow = (row: LabelGridRow): LabelGridRow["validation"] => {
    const field: Record<string, string> = {};
    for (const f of requiredForRow(row)) if ((row.data[f] ?? "").length === 0) field[f] = "required";
    return Object.keys(field).length ? { field } : {};
  };
  const rowInvalid = (row: LabelGridRow): boolean => !!validateRow(row).field;
  const viewRows = rows.map((row) => ({ ...row, validation: validateRow(row) }));
  const hasErrors = viewRows.some(rowInvalid);
  const total = expandedCount(rows.length, copies);
  const overCap = total > MAX_BATCH_LABELS;

  const addRows = async () => {
    if (selected.length === 0) return;
    setFormError(null);
    // Guard the caps BEFORE the API call: /materialize rejects > MATERIALIZE_CAP rows (400 BudgetExceeded),
    // and the grid/batch caps at MAX_BATCH_LABELS. Catch both here for a clear message and no wasted call.
    if (selected.length > MATERIALIZE_CAP) {
      setFormError(`Select at most ${MATERIALIZE_CAP} rows at a time.`);
      return;
    }
    if (rowsRef.current.length + selected.length > MAX_BATCH_LABELS) {
      setFormError(`That would exceed the ${MAX_BATCH_LABELS}-row limit.`);
      return;
    }
    setBusy(true);
    try {
      const fields = mappedConnectorKeys(mapping);
      const materialized = await materializeConnection(connectionId, { rows: selected, fields, expansion: "as_listed" });
      const built = rowsFromMaterialized(materialized, mapping, connectorId, connectionId);
      commitRows([...rowsRef.current, ...built]);
      push({ kind: "ok", message: `Added ${built.length} rows` });
    } catch (err) {
      const message = err instanceof Error ? err.message : "Materialize failed";
      setFormError(message); push({ kind: "error", message });
    } finally {
      setBusy(false);
    }
  };

  const run = async (mode: "download" | "print") => {
    setFormError(null);
    const snapshot = rowsRef.current;
    if (snapshot.length === 0) return;
    if (snapshot.some(rowInvalid)) { setFormError("Fix the highlighted rows before running."); return; }
    if (expandedCount(snapshot.length, copies) > MAX_BATCH_LABELS) { setFormError(`Too many labels (over the ${MAX_BATCH_LABELS} limit).`); return; }
    if (mode === "print" && !printer) { setFormError("Select a printer to print."); return; }
    setBusy(true);
    commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined })));
    const submittedIds = rowsRef.current.map((r) => r.id);
    const submittedCopies = copies;
    const idForExpandedIndex = (index: number): string | undefined => submittedIds[sourceRowForExpandedIndex(index, submittedCopies)];
    try {
      const labels = resolveLabels(rowsRef.current, manualOptions, submittedCopies);
      const r = await submitBatch({
        template: detail.id, labels, mode,
        ...(mode === "print" ? { printer } : {}),
        ...(isSheet && startSlot ? { start_slot: startSlot } : {}),
      });
      if (r.kind === "download") {
        saveBlob(r.blob, r.filename ?? `${detail.id}.${isSheet ? "pdf" : "zip"}`);
        push({ kind: "ok", message: `Downloaded ${labels.length} labels` });
      } else {
        const { succeeded, total: t, failed } = r.summary;
        const failById = new Map<string, string>();
        for (const f of failed) { const id = idForExpandedIndex(f.index); if (id) failById.set(id, failById.has(id) ? `${failById.get(id)}; ${f.error}` : f.error); }
        const submitted = new Set(submittedIds);
        commitRows(rowsRef.current.map((row) =>
          submitted.has(row.id)
            ? { ...row, annotation: failById.has(row.id) ? { status: "failed", message: failById.get(row.id) } : { status: "ok" } }
            : row));
        push({ kind: failed.length ? "error" : "ok", message: `Printed ${succeeded}/${t}` });
      }
    } catch (err) {
      if (err instanceof ApiError && err.code === "BatchInvalid") {
        const failures = (err.details as BatchFailures)?.failures ?? [];
        const failById = new Map<string, string>();
        for (const f of failures) { const id = idForExpandedIndex(f.index); if (id) failById.set(id, failById.has(id) ? `${failById.get(id)}; ${f.message}` : f.message); }
        commitRows(rowsRef.current.map((row) => (failById.has(row.id) ? { ...row, annotation: { status: "failed", message: failById.get(row.id) } } : row)));
        const message = failures.map((f) => f.message).join("; ") || err.message;
        setFormError(message); push({ kind: "error", message });
      } else {
        const message = err instanceof Error ? err.message : "Batch failed";
        push({ kind: "error", message });
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <section className="flex flex-col gap-2 rounded-md border p-4" style={{ borderColor: "var(--border)" }}>
        <h2 className="text-sm font-semibold">Field mapping</h2>
        <div className="flex flex-wrap gap-3">
          {templateFields.map((field) => (
            <label key={field} className="flex flex-col gap-1">
              <span className="text-xs" style={{ color: "var(--muted)" }}>{field}</span>
              <select aria-label={`map ${field}`} value={mapping[field] ?? ""} onChange={(e) => setMapping({ ...mapping, [field]: e.target.value })} className={inputClass} style={inputStyle}>
                <option value="">(blank)</option>
                {connectorKeys.map((k) => (<option key={k} value={k}>{k}</option>))}
              </select>
            </label>
          ))}
        </div>
        <div>
          <button type="button" onClick={addRows} disabled={busy || selected.length === 0} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
            Add {selected.length} rows
          </button>
        </div>
      </section>

      {rows.length > 0 && (
        <>
          {declaredNames.length > 0 && (
            <div className="flex flex-wrap gap-3">
              {declaredNames.map((name) => (
                <label key={name} className="flex flex-col gap-1">
                  <span className="text-sm font-medium">{name}</span>
                  <select aria-label={name} value={manualOptions[name] ?? declaredOptions[name][0] ?? ""} disabled={busy}
                    onChange={(e) => { setManualOptions({ ...manualOptions, [name]: e.target.value }); commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
                    className={inputClass} style={inputStyle}>
                    {declaredOptions[name].map((v) => (<option key={v} value={v}>{v}</option>))}
                  </select>
                </label>
              ))}
            </div>
          )}

          <div className="flex flex-wrap items-end gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-sm font-medium">Copies</span>
              <input type="number" min={1} aria-label="copies" value={copies} disabled={busy}
                onChange={(e) => { setCopies(Math.max(1, Math.floor(Number(e.target.value) || 1))); commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
                className={inputClass} style={inputStyle} />
            </label>
            {isSheet && (
              <label className="flex flex-col gap-1">
                <span className="text-sm font-medium">Start slot</span>
                <input type="number" min={0} max={Math.max(0, positions - 1)} aria-label="start slot" value={startSlot} disabled={busy}
                  onChange={(e) => { setStartSlot(Math.max(0, Math.min(positions - 1, Math.floor(Number(e.target.value) || 0)))); commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
                  className={inputClass} style={inputStyle} />
              </label>
            )}
            <label className="flex flex-col gap-1">
              <span className="text-sm font-medium">Printer</span>
              <select aria-label="printer" value={printer ?? ""} disabled={busy} onChange={(e) => { setPrinter(e.target.value || undefined); setFormError(null); }} className={inputClass} style={inputStyle}>
                <option value="">none (download only)</option>
                {printers.map((p) => (<option key={p.id} value={p.id}>{p.name}</option>))}
              </select>
            </label>
            <span className="text-sm" style={{ color: "var(--muted)" }}>{total} labels</span>
          </div>

          {overCap && <p style={{ color: "var(--bad)" }}>{total} labels is over the {MAX_BATCH_LABELS}-label limit. Reduce rows or copies.</p>}
          {formError && <p style={{ color: "var(--bad)" }}>{formError}</p>}

          <LabelGrid
            rows={viewRows}
            fields={displayedFields}
            optionNames={[]}
            optionValues={declaredOptions}
            onRowsChange={(next, { indexes }) => {
              const dirty = new Set(indexes);
              commitRows(next.map((r, i) => ({ ...r, validation: {}, annotation: dirty.has(i) ? undefined : r.annotation })));
              setFormError(null);
            }}
            onDuplicate={(id) => { commitRows(duplicateRow(rowsRef.current, id).map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
            onRemove={(id) => { commitRows(removeRow(rowsRef.current, id).map((r) => ({ ...r, annotation: undefined }))); setFormError(null); }}
            disabled={busy}
          />

          <div className="flex gap-3">
            <button type="button" onClick={() => run("print")} disabled={busy || overCap || hasErrors || !printer} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}>Print</button>
            <button type="button" onClick={() => run("download")} disabled={busy || overCap || hasErrors} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>Download</button>
          </div>
        </>
      )}
    </div>
  );
}
```
(The `Composer` deliberately reuses the exact run/annotation logic from `Import.tsx`'s `CsvEditor`; keep it identical so behavior matches. The only differences from Import: rows come from `materializeConnection` + `rowsFromMaterialized` instead of CSV, there are no per-row option columns (`optionNames={[]}`), and validation is the required-field check only.)

- [ ] **Step 4: Route + nav.**
  - `src/app/App.tsx`: import `Connect` and add `<Route path="connect" element={<Connect />} />` inside the `Shell` route group (next to `print`/`import`).
  - `src/app/Shell.tsx`: add `{ to: "/connect", label: "Connect" }` to `NAV_ITEMS` (after Import).

- [ ] **Step 5: Run tests** — `npm run test -- Connect` (pass), then `npm run test` (full suite green), `npm run lint`, `npm run build`.

- [ ] **Step 6: Commit**
```bash
git add ui/src/pages/Connect.tsx ui/src/pages/Connect.test.tsx ui/src/app/App.tsx ui/src/app/Shell.tsx
git commit -m "feat(ui): Connect page (browse -> map -> materialize -> batch) + route and nav"
```

---

### Task 6: Docs, review, integrate

**Files:** `docs/SPEC.md`; review; merge.

- [ ] **Step 1: SPEC** — extend the "Integrations (connectors)" section added in Plan A with a short "Using a connection (UI)" paragraph: Settings -> Connections to add a Homebox connection (base URL + API key); the Connect page to browse/drill, select rows, map fields to a template, and download/print a batch. Add a dated changelog entry (today is 2026-06-17) matching the document's format.

- [ ] **Step 2: Adversarial review** — dispatch a reviewer against `git diff main...<branch>` for the UI changes. Audit: the API key is never rendered or logged (only `has_credential` shown; the input is `type="password"`, never echoed to the table; on edit the field starts blank and a blank value omits `credential`); selection identity is stable across pagination/drill; the `materialize` request only asks for mapped keys and respects the 200/500 caps; the run/annotation logic matches `Import.tsx` (no regressions); no `fetch` calls outside the api layer; lint/build/test clean; no `any`, no em dashes. Fix every meaningful finding; re-review until clean.

- [ ] **Step 3: Gate + integrate**
```bash
(cd ui && npm ci && npm run lint && npm run test && npm run build)
cargo fmt && cargo clippy --all-targets --all-features && cargo test   # backend untouched, but confirm the workspace is green
git checkout main && git merge <branch> && git push
```
This is the second of the two M7 sub-projects; with the UI shipped, close the Homebox issue. Reference `Fixes #35` in the merge commit so it closes on push (Plan A intentionally did not).

---

## Self-Review

**1. Spec coverage:** Connections CRUD + API-key paste + redacted display -> Task 2 (+ Task 1 layer). Generic schema-driven browse (table/tree, filters search/location/label, cursor pagination, direct drill-down) -> Task 4 (+ Task 1). Field mapping connector fields -> LabelGrid -> Task 3 + Task 5. Materialize selected rows -> `/materialize` -> grid -> `/batch` PDF/print -> Task 5. Routing/nav -> Task 5. Docs/close issue -> Task 6. The "tree view" renders flat in v1 (backend flattens the tree); noted in Task 4 and acceptable per the spec's deferred-recursion decision.

**2. Placeholder scan:** no TBD/TODO; every step has complete code or an exact edit. No `eslint-disable` and no `any`. The browse effect (Task 4) lists its real deps `[connectionId, resource, applied, parent]` (`resource` is memoized on `resourceId`) and uses an `active` cancellation flag so a stale request cannot overwrite a newer one; "Load more" is a separate `loadMore`. `addRows` (Task 5) guards both the 200-row `/materialize` cap (`MATERIALIZE_CAP`) and the 500-row batch cap before calling the API.

**3. Type/name consistency:** the api types (`Connection`, `ConnectorSchema`, `ResourceSpec`, `FieldSpec`, `FilterSpec`, `RelationshipSpec`, `DisplayRow`, `RowRef`, `BrowsePage`, `LabelRowResult`) defined in Task 1 are used unchanged in Tasks 2/4/5. `browseConnection`/`materializeConnection` signatures match their callers. `FieldMapping`, `defaultMapping`, `mappedConnectorKeys`, `rowsFromMaterialized` (Task 3) are consumed by Task 5 with matching shapes. `LabelGridRow` (`origin: "connector"`, `source: RowSource`) matches the existing `src/lib/labelGrid.ts` model. `LabelGrid` props (`rows`, `fields`, `optionNames`, `optionValues`, `onRowsChange`, `onDuplicate`, `onRemove`, `disabled`) match the existing component; Task 5 passes `optionNames={[]}` (connector rows have no per-row option columns).

**Known v1 limitations (intentional, per spec):** location tree shows flat (no depth); only Direct drill-down (no recursive); AsListed expansion only (no quantity-expansion); one connector type (`homebox`) in the connector dropdown of the connection form.
