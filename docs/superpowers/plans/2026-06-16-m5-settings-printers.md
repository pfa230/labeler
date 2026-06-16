# Settings & Printers Screen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the M5 Settings & Printers screen (#23): one page with a settings key/value editor and a printers CRUD table, against the existing backend endpoints.

**Architecture:** Frontend-only (no backend change). Two independent sections compose into the `/settings` page. The data layer adds React Query hooks (`useSettings`, `useUpsertSetting`, `useSavePrinter`, `useDeletePrinter`) and a `del` client helper for the 204-returning DELETE. Each section owns its form state; mutations invalidate the relevant query cache and toast on success/error.

**Tech Stack:** React 19 + TypeScript, Vite, Tailwind v3, @tanstack/react-query 5, Vitest 4 + React Testing Library.

---

## Context the implementer needs

- **Design (source of truth):** `docs/superpowers/specs/2026-06-15-m5-web-ui-design.md`, section **§5 "Settings & Printers, `/settings` (#23)"**: "One page, two sections. *Settings*: key/value editor over `GET /api/settings` (all) + `PUT /api/settings/{key}` (upsert). The store does not auto-seed; the UI shows `qr_base_url` as a suggested/default row to fill if absent. *Printers*: table with add/edit/delete (`/api/printers` CRUD), kind + config (cups uri), enabled toggle."
- **Backend contracts (already shipped; do not change them):**
  - `GET /api/settings` returns all settings as a JSON object `{ "<key>": "<value>", ... }` (string to string).
  - `PUT /api/settings/{key}` body `{ "value": "<string>" }` returns `200 { "value": "<string>" }`. The key must be non-empty and match `[A-Za-z0-9_.-]+` (server returns `400` otherwise).
  - `GET /api/printers` returns `Printer[]`. `POST /api/printers` body `Printer` returns `201` (or `409` if the id exists, `422` if invalid). `PUT /api/printers/{id}` body `Printer` (the body `id` must equal the path id) returns `200` (`404` if missing, `400` on id mismatch, `422` if invalid). `DELETE /api/printers/{id}` returns **`204` with no body** (`404` if missing).
  - `Printer = { id: string; name: string; kind: string; config: unknown; enabled: boolean }`. The only production `kind` is `"cups"`, whose `config` is `{ "uri": "<string>" }`. The server validates: `id` matches `[A-Za-z0-9_-]+` and is non-empty, `name` non-empty (trimmed), and a `cups` config must contain a `uri`.
  - Errors follow the stable `{ error: { code, message, details } }` schema; `ApiError` in the client carries `code`/`status`/`message`/`details`.
- **Existing client/query plumbing** (`ui/src/api/`):
  - `client.ts`: `getJson<T>(path)`, `sendJson<T>(method, path, body)` (both throw `ApiError` on non-2xx; `sendJson` reads the JSON response body), and `ApiError`. There is **no** DELETE helper yet (this plan adds `del`).
  - `queries.ts`: `usePrinters()` already exists (`useQuery(["printers"], () => getJson<Printer[]>("/printers"))`); `useCreateTemplate()` is the canonical `useMutation` + `useQueryClient().invalidateQueries` pattern to mirror.
  - `types.ts`: `Printer` is already declared (`{ id; name; kind; config: unknown; enabled }`).
- **Existing UI patterns to mirror:** `ui/src/pages/print/PrintForm.tsx` (form state, `useToast()`, error mapping), `ui/src/pages/NewTemplate.tsx` (mutation + inline error + toast), `ui/src/pages/Print.test.tsx` (the `stubFetch()` + `MemoryRouter` + `QueryClientProvider` + `ToastProvider` test harness; `ToastProvider` is imported from `../app/toast`, `useToast` from `../app/toast-context`).
- **The route already exists:** `ui/src/app/App.tsx` has `<Route path="settings" element={<Settings />} />`, and the sidebar already links `/settings`. This plan only replaces the `Settings` page body; no routing change.
- **Lint constraints (CI fails otherwise):** no `any` (use `unknown` + narrowing), `noUnusedLocals`/`noUnusedParameters`, `react-hooks/set-state-in-effect` (no synchronous `setState` in an effect body), `react-refresh/only-export-components` (a `.tsx` may export only components; types/helpers live in `.ts` or are non-exported), `verbatimModuleSyntax` (mark type-only imports with `type`). No em dashes in code or docs. Never add `eslint-disable`; fix the root cause.
- **No backend or `docs/SPEC.md` API change.** SPEC gets only a changelog line (mirroring ADR-0013/#20). The work happens on a short-lived branch `m5-settings-printers`, merged to `main` in the final task.
- **Reuses established in-repo patterns.** The data layer and tests follow React Query 5 + React Testing Library patterns already present verbatim: `useMutation` + `useQueryClient().invalidateQueries` as in `ui/src/api/queries.ts` (`useCreateTemplate`), and the `stubFetch` + `QueryClientProvider` + `ToastProvider` harness as in `ui/src/pages/Print.test.tsx`. Task 0 still runs the repo-mandated current-source check before implementation.

## File structure

| File | Responsibility |
| --- | --- |
| `ui/src/api/client.ts` (modify) | Add `del(path)` for the 204 DELETE (no body parse). |
| `ui/src/api/queries.ts` (modify) | Add `useSettings`, `useUpsertSetting`, `useSavePrinter`, `useDeletePrinter`; import `sendJson`/`del`. |
| `ui/src/pages/settings/SettingsSection.tsx` (create) | Settings key/value editor: one editable row per setting, a suggested `qr_base_url` row when absent, and an add-custom-setting row. |
| `ui/src/pages/settings/PrintersSection.tsx` (create) | Printers table + add/edit form (kind `cups`, config `{uri}`, enabled) + inline delete confirm. |
| `ui/src/pages/Settings.tsx` (replace stub) | Compose the two sections under a page heading. |
| `ui/src/pages/Settings.test.tsx` (create) | Integration tests with stubbed fetch. |
| `docs/adr/0015-settings-printers-ux.md` (create) | ADR for the screen's UX decisions. |
| `docs/adr/README.md` (modify) | Index row for ADR-0015. |
| `docs/SPEC.md` (modify) | Changelog entry (no API change). |
| `docs/PLAN-phase-1.md` (modify, in the review task) | Mark P1-55 DONE. |

---

### Task 0: Branch setup

- [ ] **Step 1: Create the short-lived feature branch**

```bash
git checkout main && git pull && git checkout -b m5-settings-printers
```
All task commits are local checkpoints on `m5-settings-printers` (WIP); nothing is integrated until Task 6's adversarial review passes and its fixes are committed. The final task merges to `main`.

- [ ] **Step 2: Confirm current library behavior (repo-mandated web check)**

Per the repo rule ("run at least one web search first for non-trivial work"), do a quick current-source check and note the findings in the implementation, confirming for the pinned versions in `ui/package.json`:
- React Query 5: `useMutation`'s `mutate(vars, { onSuccess, onError })` per-call callbacks fire in addition to the hook's options, and `queryClient.invalidateQueries({ queryKey })` triggers a refetch of active queries.
- React Testing Library + Vitest 4: `findBy*` async queries, `within`, and `waitFor` usage match what `ui/src/pages/Print.test.tsx` already does.

If any behavior differs from the plan's assumptions, flag it before proceeding rather than coding against a stale API.

---

### Task 1: Data layer (DELETE helper + query/mutation hooks)

This is enabling plumbing exercised end-to-end by the Task 2-4 component tests; verification here is typecheck + lint + existing suite staying green.

**Files:**
- Modify: `ui/src/api/client.ts`
- Modify: `ui/src/api/queries.ts`

- [ ] **Step 1: Add the `del` helper to `client.ts`**

Append to `ui/src/api/client.ts` (it reuses the module's `BASE` and `toError`):
```ts
// DELETE returns 204 with no body, so there is nothing to parse; throw the error contract on non-2xx.
export async function del(path: string): Promise<void> {
  const res = await fetch(`${BASE}${path}`, { method: "DELETE" });
  if (!res.ok) throw await toError(res);
}
```

- [ ] **Step 2: Add the hooks to `queries.ts`**

Update the imports at the top of `ui/src/api/queries.ts` from:
```ts
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getJson } from "./client"; // only getJson is used here; do NOT import sendJson (noUnusedLocals)
import type { TemplateSummary, TemplateDetail, Printer } from "./types";
```
to:
```ts
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getJson, sendJson, del } from "./client";
import type { TemplateSummary, TemplateDetail, Printer } from "./types";
```
Then append these hooks at the end of the file:
```ts
export function useSettings() {
  return useQuery({ queryKey: ["settings"], queryFn: () => getJson<Record<string, string>>("/settings") });
}

export function useUpsertSetting() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ key, value }: { key: string; value: string }) =>
      sendJson<{ value: string }>("PUT", `/settings/${encodeURIComponent(key)}`, { value }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
}

export function useSavePrinter() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ printer, isNew }: { printer: Printer; isNew: boolean }) =>
      isNew
        ? sendJson<Printer>("POST", "/printers", printer)
        : sendJson<Printer>("PUT", `/printers/${encodeURIComponent(printer.id)}`, printer),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["printers"] }),
  });
}

export function useDeletePrinter() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => del(`/printers/${encodeURIComponent(id)}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["printers"] }),
  });
}
```

- [ ] **Step 3: Verify lint, types, and the existing suite**

Run (from `ui/`): `npm run lint && npm run test && npm run build`
Expected: lint clean, all existing tests pass, build succeeds (this typechecks the new hooks).

- [ ] **Step 4: Commit**

```bash
git add ui/src/api/client.ts ui/src/api/queries.ts
git commit -m "feat(ui): settings/printers query + mutation hooks and del helper"
```

---

### Task 2: Settings section

**Files:**
- Create: `ui/src/pages/settings/SettingsSection.tsx`
- Test: `ui/src/pages/settings/SettingsSection.test.tsx`

The section lists every stored setting as an editable row, always shows `qr_base_url` (suggested) even when absent, and offers an add-custom-setting row. Each row has its own value state and Save button (one `PUT /settings/{key}`).

- [ ] **Step 1: Write the failing test**

Create `ui/src/pages/settings/SettingsSection.test.tsx`:
```tsx
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { SettingsSection } from "./SettingsSection";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

// Stateful stub: PUT mutates `settings` and GET returns a fresh copy, so an invalidate+refetch reflects
// the saved value (which is what makes a saved row stop being dirty).
function stubFetch(settings: Record<string, string>) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/settings/")) {
      const key = decodeURIComponent(url.slice("/api/settings/".length));
      const value = JSON.parse(init!.body as string).value as string;
      settings[key] = value;
      return json({ value });
    }
    if (url.startsWith("/api/settings")) return json({ ...settings });
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderSection() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <SettingsSection />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

let fetchMock: ReturnType<typeof stubFetch>;
const lastCall = (path: string) => [...fetchMock.mock.calls].reverse().find(([u]) => String(u).startsWith(path));
const settingsGets = () => fetchMock.mock.calls.filter(([u]) => String(u) === "/api/settings").length;

describe("SettingsSection", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("shows existing settings and the suggested qr_base_url row when absent", async () => {
    fetchMock = stubFetch({ company: "Acme" });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    expect(await screen.findByLabelText("company")).toHaveValue("Acme");
    // qr_base_url is suggested even though it is not stored
    const qr = (await screen.findByLabelText("qr_base_url")) as HTMLInputElement;
    expect(qr.value).toBe("");
    expect(screen.getByText(/suggested/i)).toBeInTheDocument();
  });

  it("saves an edited setting via PUT /settings/{key}", async () => {
    fetchMock = stubFetch({ company: "Acme" });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    const input = (await screen.findByLabelText("company")) as HTMLInputElement;
    const getsBefore = settingsGets();
    fireEvent.change(input, { target: { value: "Globex" } });
    fireEvent.click(screen.getByRole("button", { name: /save company/i }));
    await waitFor(() => expect(lastCall("/api/settings/company")).toBeTruthy());
    const call = lastCall("/api/settings/company")!;
    expect((call[1] as RequestInit).method).toBe("PUT");
    expect(JSON.parse((call[1] as RequestInit).body as string)).toEqual({ value: "Globex" });
    // Wait for the post-save refetch (so the mutation has settled and isPending is false), then the row
    // is disabled because draft === the refetched value (clean), not merely because it is pending.
    await waitFor(() => expect(settingsGets()).toBeGreaterThan(getsBefore));
    expect(screen.getByRole("button", { name: /save company/i })).toBeDisabled();
  });

  it("adds a custom setting and rejects an invalid key client-side", async () => {
    fetchMock = stubFetch({});
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    fireEvent.change(await screen.findByLabelText(/new setting key/i), { target: { value: "bad key" } });
    fireEvent.change(screen.getByLabelText(/new setting value/i), { target: { value: "x" } });
    fireEvent.click(screen.getByRole("button", { name: /add setting/i }));
    expect(await screen.findByText(/must be non-empty and contain only/i)).toBeInTheDocument();
    expect([...fetchMock.mock.calls].some(([u]) => String(u).startsWith("/api/settings/"))).toBe(false);

    fireEvent.change(screen.getByLabelText(/new setting key/i), { target: { value: "label_dpi" } });
    fireEvent.click(screen.getByRole("button", { name: /add setting/i }));
    await waitFor(() => expect(lastCall("/api/settings/label_dpi")).toBeTruthy());
  });

  it("rejects adding a key that already exists (would strand its row)", async () => {
    fetchMock = stubFetch({ qr_base_url: "https://x" });
    vi.stubGlobal("fetch", fetchMock);
    renderSection();
    await screen.findByLabelText("qr_base_url");
    fireEvent.change(screen.getByLabelText(/new setting key/i), { target: { value: "qr_base_url" } });
    fireEvent.change(screen.getByLabelText(/new setting value/i), { target: { value: "y" } });
    fireEvent.click(screen.getByRole("button", { name: /add setting/i }));
    expect(await screen.findByText(/already exists/i)).toBeInTheDocument();
    expect([...fetchMock.mock.calls].some(([u]) => String(u).startsWith("/api/settings/qr_base_url"))).toBe(false);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run (from `ui/`): `npm run test -- SettingsSection`
Expected: FAIL ("Cannot find module './SettingsSection'").

- [ ] **Step 3: Implement `ui/src/pages/settings/SettingsSection.tsx`**

```tsx
import { useState } from "react";
import { useSettings, useUpsertSetting } from "../../api/queries";
import { useToast } from "../../app/toast-context";

const SUGGESTED_KEYS = ["qr_base_url"]; // not auto-seeded by the store; shown so the user can fill them
const KEY_RE = /^[A-Za-z0-9_.-]+$/; // mirrors the server's accepted setting-key charset

const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

function SettingRow({ settingKey, value, suggested }: { settingKey: string; value: string; suggested: boolean }) {
  const [draft, setDraft] = useState(value);
  const upsert = useUpsertSetting();
  const { push } = useToast();
  const dirty = draft !== value;
  return (
    <div className="flex items-end gap-3">
      <label className="flex flex-1 flex-col gap-1">
        <span className="font-mono text-sm font-medium">
          {settingKey}
          {suggested && <span className="ml-2 text-xs" style={{ color: "var(--muted)" }}>(suggested)</span>}
        </span>
        <input
          aria-label={settingKey}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          className={inputClass}
          style={inputStyle}
        />
      </label>
      <button
        type="button"
        aria-label={`save ${settingKey}`}
        disabled={!dirty || upsert.isPending}
        onClick={() =>
          upsert.mutate(
            { key: settingKey, value: draft },
            {
              onSuccess: () => push({ kind: "ok", message: `Saved ${settingKey}` }),
              onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Save failed" }),
            },
          )
        }
        className={`${buttonBase} border`}
        style={{ borderColor: "var(--border)", color: "var(--ink)" }}
      >
        Save
      </button>
    </div>
  );
}

export function SettingsSection() {
  const { data: settings, isError } = useSettings();
  const upsert = useUpsertSetting();
  const { push } = useToast();
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");
  const [keyError, setKeyError] = useState<string | null>(null);

  // Render rows only once settings have loaded: SettingRow seeds its draft from `value` on mount and is
  // keyed by the stable setting key, so mounting a row before the real value arrives would strand it at "".
  if (settings === undefined) {
    return (
      <section className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold">General settings</h2>
        <p className="text-sm" style={{ color: isError ? "var(--bad)" : "var(--muted)" }}>
          {isError ? "Failed to load settings." : "Loading settings..."}
        </p>
      </section>
    );
  }

  const stored = settings;
  const keys = [...new Set([...Object.keys(stored), ...SUGGESTED_KEYS])].sort();

  const addSetting = () => {
    if (!KEY_RE.test(newKey)) {
      setKeyError("key must be non-empty and contain only letters, digits, '_', '-' or '.'");
      return;
    }
    if (keys.includes(newKey)) {
      // Adding an already-displayed key (including a suggested one) would strand its existing row, so
      // route the user to that row instead of creating a second, out-of-sync row.
      setKeyError("setting already exists; edit its row above");
      return;
    }
    setKeyError(null);
    upsert.mutate(
      { key: newKey, value: newValue },
      {
        onSuccess: () => {
          push({ kind: "ok", message: `Saved ${newKey}` });
          setNewKey("");
          setNewValue("");
        },
        onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Save failed" }),
      },
    );
  };

  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-lg font-semibold">General settings</h2>
      <div className="flex flex-col gap-3">
        {keys.map((key) => (
          <SettingRow key={key} settingKey={key} value={stored[key] ?? ""} suggested={!(key in stored)} />
        ))}
      </div>

      <div className="flex flex-col gap-2 border-t pt-4" style={{ borderColor: "var(--border)" }}>
        <span className="text-sm font-medium">Add a setting</span>
        <div className="flex items-end gap-3">
          <label className="flex flex-col gap-1">
            <span className="text-xs" style={{ color: "var(--muted)" }}>new setting key</span>
            <input aria-label="new setting key" value={newKey} onChange={(e) => setNewKey(e.target.value)} className={inputClass} style={inputStyle} />
          </label>
          <label className="flex flex-1 flex-col gap-1">
            <span className="text-xs" style={{ color: "var(--muted)" }}>new setting value</span>
            <input aria-label="new setting value" value={newValue} onChange={(e) => setNewValue(e.target.value)} className={inputClass} style={inputStyle} />
          </label>
          <button type="button" onClick={addSetting} disabled={upsert.isPending} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
            Add setting
          </button>
        </div>
        {keyError && <p className="text-sm" style={{ color: "var(--bad)" }}>{keyError}</p>}
      </div>
    </section>
  );
}
```

> `SettingRow` initializes `draft` from `value` once. After a successful save the `settings` query is invalidated and refetched, so the row's `value` prop becomes the saved string; since `draft` already equals it, the row is no longer dirty (Save disables) without an effect or remount. Rows are keyed by `settingKey` (stable), so unsaved drafts in other rows survive a refetch.

- [ ] **Step 4: Run to verify it passes**

Run (from `ui/`): `npm run test -- SettingsSection`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add ui/src/pages/settings/SettingsSection.tsx ui/src/pages/settings/SettingsSection.test.tsx
git commit -m "feat(ui): settings key/value editor section"
```

---

### Task 3: Printers section

**Files:**
- Create: `ui/src/pages/settings/PrintersSection.tsx`
- Test: `ui/src/pages/settings/PrintersSection.test.tsx`

A table of printers with Edit/Delete per row, an Add button, and a form (id, name, kind, cups uri, enabled). Delete uses a two-step inline confirm (testable; no `window.confirm`).

- [ ] **Step 1: Write the failing test**

Create `ui/src/pages/settings/PrintersSection.test.tsx`:
```tsx
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../../app/toast";
import { PrintersSection } from "./PrintersSection";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

type P = { id: string; name: string; kind: string; config: { uri: string }; enabled: boolean };

// Stateful stub: POST/PUT/DELETE mutate `state`, GET returns it, so an invalidate+refetch shows the
// real post-mutation table (this is what proves add/edit/delete actually took effect, and that the
// 204 DELETE path in `del` does not try to parse a body).
function stubFetch() {
  let state: P[] = [{ id: "front", name: "Front Desk", kind: "cups", config: { uri: "ipp://x/y" }, enabled: true }];
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.startsWith("/api/printers/") && method === "DELETE") {
      const id = decodeURIComponent(url.slice("/api/printers/".length));
      state = state.filter((p) => p.id !== id);
      return new Response(null, { status: 204 });
    }
    if (url.startsWith("/api/printers/") && method === "PUT") {
      const p = JSON.parse(init!.body as string) as P;
      state = state.map((x) => (x.id === p.id ? p : x));
      return json(p);
    }
    if (url.startsWith("/api/printers") && method === "POST") {
      const p = JSON.parse(init!.body as string) as P;
      state = [...state, p];
      return json(p, 201);
    }
    if (url.startsWith("/api/printers")) return json(state);
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderSection() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <PrintersSection />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

let fetchMock: ReturnType<typeof stubFetch>;
const lastCall = (path: string, method: string) =>
  [...fetchMock.mock.calls].reverse().find(([u, i]) => String(u).startsWith(path) && ((i as RequestInit)?.method ?? "GET").toUpperCase() === method);

describe("PrintersSection", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("lists printers with name, kind and uri", async () => {
    renderSection();
    expect(await screen.findByText("Front Desk")).toBeInTheDocument();
    expect(screen.getByText("ipp://x/y")).toBeInTheDocument();
  });

  it("adds a printer via POST with a cups config", async () => {
    renderSection();
    await screen.findByText("Front Desk");
    fireEvent.click(screen.getByRole("button", { name: /add printer/i }));
    fireEvent.change(screen.getByLabelText(/printer id/i), { target: { value: "back" } });
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "Back Office" } });
    fireEvent.change(screen.getByLabelText(/cups uri/i), { target: { value: "ipp://b/q" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(lastCall("/api/printers", "POST")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/printers", "POST")![1] as RequestInit).body as string);
    expect(body).toEqual({ id: "back", name: "Back Office", kind: "cups", config: { uri: "ipp://b/q" }, enabled: true });
    // the new printer appears in the refetched table
    expect(await screen.findByText("Back Office")).toBeInTheDocument();
  });

  it("edits a printer via PUT with an immutable id", async () => {
    renderSection();
    const row = (await screen.findByText("Front Desk")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /edit/i }));
    expect(screen.getByLabelText(/printer id/i)).toBeDisabled(); // id is immutable on edit
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "Lobby" } });
    fireEvent.change(screen.getByLabelText(/cups uri/i), { target: { value: "ipp://x/z" } });
    fireEvent.click(screen.getByLabelText("enabled")); // toggle true -> false
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(lastCall("/api/printers/front", "PUT")).toBeTruthy());
    const body = JSON.parse((lastCall("/api/printers/front", "PUT")![1] as RequestInit).body as string);
    expect(body).toEqual({ id: "front", name: "Lobby", kind: "cups", config: { uri: "ipp://x/z" }, enabled: false });
    expect(await screen.findByText("Lobby")).toBeInTheDocument();
  });

  it("blocks an invalid printer id client-side", async () => {
    renderSection();
    await screen.findByText("Front Desk");
    fireEvent.click(screen.getByRole("button", { name: /add printer/i }));
    fireEvent.change(screen.getByLabelText(/printer id/i), { target: { value: "bad id" } });
    fireEvent.change(screen.getByLabelText(/printer name/i), { target: { value: "X" } });
    fireEvent.change(screen.getByLabelText(/cups uri/i), { target: { value: "ipp://b/q" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByText(/id must contain only/i)).toBeInTheDocument();
    expect(lastCall("/api/printers", "POST")).toBeUndefined();
  });

  it("cancels then deletes a printer after an inline confirm", async () => {
    renderSection();
    const row = (await screen.findByText("Front Desk")).closest("tr") as HTMLElement;
    // Delete -> Cancel: no DELETE issued, row stays.
    fireEvent.click(within(row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /cancel/i }));
    expect(lastCall("/api/printers/front", "DELETE")).toBeUndefined();
    expect(screen.getByText("Front Desk")).toBeInTheDocument();
    // Delete -> Confirm: DELETE issued (204, no body), row removed after the refetch.
    fireEvent.click(within(row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /confirm/i }));
    await waitFor(() => expect(lastCall("/api/printers/front", "DELETE")).toBeTruthy());
    await waitFor(() => expect(screen.queryByText("Front Desk")).not.toBeInTheDocument());
  });

  it("closes the edit form when the printer being edited is deleted", async () => {
    renderSection();
    const row = (await screen.findByText("Front Desk")).closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: /edit/i }));
    expect(screen.getByLabelText(/printer id/i)).toBeInTheDocument(); // form is open
    fireEvent.click(within(row).getByRole("button", { name: /^delete$/i }));
    fireEvent.click(within(row).getByRole("button", { name: /confirm/i }));
    // deleting the edited printer closes the now-stale form (otherwise Save would PUT a 404).
    await waitFor(() => expect(screen.queryByLabelText(/printer id/i)).not.toBeInTheDocument());
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run (from `ui/`): `npm run test -- PrintersSection`
Expected: FAIL ("Cannot find module './PrintersSection'").

- [ ] **Step 3: Implement `ui/src/pages/settings/PrintersSection.tsx`**

```tsx
import { useState } from "react";
import { usePrinters, useSavePrinter, useDeletePrinter } from "../../api/queries";
import { useToast } from "../../app/toast-context";
import type { Printer } from "../../api/types";

const ID_RE = /^[A-Za-z0-9_-]+$/; // mirrors the server's accepted printer-id charset
const inputClass = "w-full rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;
const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

function cupsUri(p: Printer): string {
  // config is `unknown`; narrow with a guard (not an assertion) and only accept a string uri.
  const config = p.config;
  if (typeof config === "object" && config !== null && "uri" in config) {
    const uri = (config as { uri?: unknown }).uri;
    if (typeof uri === "string") return uri;
  }
  return "";
}

function PrinterForm({ initial, onClose }: { initial: Printer | null; onClose: () => void }) {
  const isNew = initial === null;
  const [id, setId] = useState(initial?.id ?? "");
  const [name, setName] = useState(initial?.name ?? "");
  const [uri, setUri] = useState(initial ? cupsUri(initial) : "");
  const [enabled, setEnabled] = useState(initial?.enabled ?? true);
  const [error, setError] = useState<string | null>(null);
  const save = useSavePrinter();
  const { push } = useToast();

  const submit = () => {
    if (!ID_RE.test(id)) {
      setError("id must contain only letters, digits, '-' or '_'");
      return;
    }
    if (name.trim() === "") {
      setError("name must not be empty");
      return;
    }
    if (uri.trim() === "") {
      setError("cups uri must not be empty");
      return;
    }
    setError(null);
    const printer: Printer = { id, name, kind: "cups", config: { uri }, enabled };
    save.mutate(
      { printer, isNew },
      {
        onSuccess: () => {
          push({ kind: "ok", message: `Saved ${id}` });
          onClose();
        },
        onError: (err) => {
          const message = err instanceof Error ? err.message : "Save failed";
          setError(message);
          push({ kind: "error", message });
        },
      },
    );
  };

  return (
    <div className="flex flex-col gap-3 rounded-md border p-4" style={{ borderColor: "var(--border)" }}>
      <div className="flex flex-wrap gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>printer id</span>
          <input aria-label="printer id" value={id} disabled={!isNew} onChange={(e) => setId(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>printer name</span>
          <input aria-label="printer name" value={name} onChange={(e) => setName(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>kind</span>
          <select aria-label="printer kind" value="cups" disabled className={inputClass} style={inputStyle}>
            <option value="cups">cups</option>
          </select>
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-xs" style={{ color: "var(--muted)" }}>cups uri</span>
          <input aria-label="cups uri" value={uri} onChange={(e) => setUri(e.target.value)} className={inputClass} style={inputStyle} />
        </label>
        <label className="flex items-center gap-2 self-end pb-2">
          <input type="checkbox" aria-label="enabled" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
          <span className="text-sm">enabled</span>
        </label>
      </div>
      {error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
      <div className="flex gap-3">
        <button type="button" onClick={submit} disabled={save.isPending} className={buttonBase} style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}>
          Save
        </button>
        <button type="button" onClick={onClose} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
          Cancel
        </button>
      </div>
    </div>
  );
}

function PrinterRow({ printer, onEdit, onDeleted }: { printer: Printer; onEdit: () => void; onDeleted: (id: string) => void }) {
  const [confirming, setConfirming] = useState(false);
  const remove = useDeletePrinter();
  const { push } = useToast();
  const td = "px-3 py-2 text-sm";
  return (
    <tr style={{ borderTop: "1px solid var(--border)" }}>
      <td className={td}>{printer.name}</td>
      <td className={`${td} font-mono`}>{printer.kind}</td>
      <td className={`${td} font-mono`}>{cupsUri(printer)}</td>
      <td className={td}>{printer.enabled ? "yes" : "no"}</td>
      <td className={`${td} flex gap-2`}>
        <button type="button" onClick={onEdit} className="underline" style={{ color: "var(--ink)" }}>Edit</button>
        {confirming ? (
          <>
            <button
              type="button"
              disabled={remove.isPending}
              onClick={() =>
                remove.mutate(printer.id, {
                  onSuccess: () => {
                    push({ kind: "ok", message: `Deleted ${printer.id}` });
                    onDeleted(printer.id);
                  },
                  onError: (err) => push({ kind: "error", message: err instanceof Error ? err.message : "Delete failed" }),
                })
              }
              style={{ color: "var(--bad)" }}
            >
              Confirm
            </button>
            <button type="button" onClick={() => setConfirming(false)} style={{ color: "var(--muted)" }}>Cancel</button>
          </>
        ) : (
          <button type="button" onClick={() => setConfirming(true)} style={{ color: "var(--bad)" }}>Delete</button>
        )}
      </td>
    </tr>
  );
}

export function PrintersSection() {
  const { data: printers, isPending, isError } = usePrinters();
  const [editing, setEditing] = useState<Printer | "new" | null>(null);
  const th = "px-3 py-2 text-left text-xs font-medium";
  // If the printer currently being edited is deleted, close the now-stale form (a Save would 404).
  const onDeleted = (id: string) => {
    if (editing !== null && editing !== "new" && editing.id === id) setEditing(null);
  };

  return (
    <section className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Printers</h2>
        <button
          type="button"
          onClick={() => setEditing("new")}
          className={`${buttonBase} border`}
          style={{ borderColor: "var(--border)", color: "var(--ink)" }}
        >
          Add printer
        </button>
      </div>

      {editing !== null && (
        <PrinterForm
          key={editing === "new" ? "new" : editing.id}
          initial={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
        />
      )}

      {isPending ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>Loading printers...</p>
      ) : isError ? (
        <p className="text-sm" style={{ color: "var(--bad)" }}>Failed to load printers.</p>
      ) : (printers ?? []).length === 0 ? (
        <p className="text-sm" style={{ color: "var(--muted)" }}>No printers configured.</p>
      ) : (
        <table className="w-full border-collapse">
          <thead>
            <tr>
              <th className={th} style={{ color: "var(--muted)" }}>Name</th>
              <th className={th} style={{ color: "var(--muted)" }}>Kind</th>
              <th className={th} style={{ color: "var(--muted)" }}>URI</th>
              <th className={th} style={{ color: "var(--muted)" }}>Enabled</th>
              <th className={th} style={{ color: "var(--muted)" }}></th>
            </tr>
          </thead>
          <tbody>
            {(printers ?? []).map((p) => (
              <PrinterRow key={p.id} printer={p} onEdit={() => setEditing(p)} onDeleted={onDeleted} />
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
```

> The `PrinterForm` is keyed by the target (`"new"` or the printer id), so switching the edit target directly from printer A to printer B (both Edit buttons stay visible while the form is open) remounts the form and re-runs its `useState` initializers with the new printer's values. Without the key, React would reuse the instance and keep A's stale field values.

- [ ] **Step 4: Run to verify it passes**

Run (from `ui/`): `npm run test -- PrintersSection`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add ui/src/pages/settings/PrintersSection.tsx ui/src/pages/settings/PrintersSection.test.tsx
git commit -m "feat(ui): printers CRUD section"
```

---

### Task 4: Compose the Settings page

**Files:**
- Replace: `ui/src/pages/Settings.tsx`
- Test: `ui/src/pages/Settings.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `ui/src/pages/Settings.test.tsx`:
```tsx
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { Settings } from "./Settings";

const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });

function stubFetch() {
  return vi.fn(async (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/settings")) return json({ qr_base_url: "https://x" });
    if (url.startsWith("/api/printers")) return json([]);
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <Settings />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("Settings page", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.stubGlobal("fetch", stubFetch());
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("renders both sections", async () => {
    renderPage();
    expect(await screen.findByRole("heading", { level: 1, name: /^settings$/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /printers/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /general settings/i })).toBeInTheDocument();
    // settings section loaded the stored qr_base_url value
    expect(await screen.findByLabelText("qr_base_url")).toHaveValue("https://x");
    // printers empty state
    expect(await screen.findByText(/no printers configured/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run (from `ui/`): `npm run test -- "pages/Settings"`
Expected: FAIL (the current stub renders only an `<h1>Settings</h1>` with no sections).

- [ ] **Step 3: Replace `ui/src/pages/Settings.tsx`**

```tsx
import { SettingsSection } from "./settings/SettingsSection";
import { PrintersSection } from "./settings/PrintersSection";

export function Settings() {
  return (
    <div className="flex flex-col gap-8">
      <h1 className="text-2xl font-semibold">Settings</h1>
      <SettingsSection />
      <PrintersSection />
    </div>
  );
}
```

- [ ] **Step 4: Run to verify it passes**

Run (from `ui/`): `npm run test -- "pages/Settings"`
Expected: PASS (1 test).

- [ ] **Step 5: Run the full UI gate**

Run (from `ui/`): `npm run lint && npm run test && npm run build`
Expected: lint clean, all tests pass, build succeeds.

- [ ] **Step 6: Commit**

```bash
git add ui/src/pages/Settings.tsx ui/src/pages/Settings.test.tsx
git commit -m "feat(ui): compose Settings & Printers page (#23)

Fixes #23"
```
This commit carries `Fixes #23` so the issue closes when the branch reaches `main`.

---

### Task 5: Documentation (ADR + SPEC changelog)

**Files:**
- Create: `docs/adr/0015-settings-printers-ux.md`
- Modify: `docs/adr/README.md`
- Modify: `docs/SPEC.md`

> P1-55 is marked DONE in Task 6 after the review loop, so its commit hash points at the final reviewed work.

- [ ] **Step 1: Write ADR-0015**

Create `docs/adr/0015-settings-printers-ux.md`:
```markdown
# 15. Settings & Printers screen UX

**Status:** Accepted

## Context

ADR-0008 named a settings/printers screen as part of M5; building it (issue #23) fixed a few UX choices
that interact with the backend contracts (`GET /api/settings`, `PUT /api/settings/{key}`, `/api/printers`
CRUD). They are recorded here so the screen stays consistent with the rest of the UI.

## Decision

- **Settings is a flat key/value editor.** One editable row per stored setting; each row owns its draft
  value and saves with a single `PUT /api/settings/{key}`. Keys not yet stored but expected (currently
  `qr_base_url`) are shown as suggested rows so the user can fill them, since the store does not auto-seed.
  An add-custom-setting row creates arbitrary keys, validated client-side against the server charset
  (`[A-Za-z0-9_.-]+`). There is no delete (the backend exposes upsert only).
- **Printers is a CRUD table.** Add/edit via a form (id, name, kind, config, enabled); delete via a
  two-step inline confirm (no `window.confirm`, so it is testable and non-blocking). The id is immutable
  on edit (it is the path key; editing posts a `PUT /api/printers/{id}`); creating posts `POST /api/printers`.
- **Printer kind is fixed to `cups`** in the form, the only production driver; its config is `{ uri }`.
  The kind select is present but disabled, leaving room for more driver kinds later without reworking the
  form. Client-side validation mirrors the server (id charset, non-empty name, non-empty uri) so common
  mistakes are caught before the request; server errors still surface inline and as a toast.

## Consequences

- No backend or API change; the screen consumes existing endpoints. SPEC gets a changelog entry only.
- Adding a second printer driver kind later means enabling the kind select and a small per-kind config
  sub-form; the `Printer.config` stays an opaque object in the client.

## Alternatives considered

- **`window.confirm` for delete.** Rejected: blocks the event loop and is awkward to test; an inline
  confirm row is clearer and testable.
- **A schema-driven settings form.** Rejected as premature: settings are free-form string pairs in Phase
  1, so a flat editor plus suggested keys is enough.
```

- [ ] **Step 2: Add the index row**

In `docs/adr/README.md`, after the ADR-0014 row, add:
```markdown
| [0015](0015-settings-printers-ux.md) | Settings & Printers screen UX | Accepted |
```

- [ ] **Step 3: Add the SPEC changelog entry**

In `docs/SPEC.md`, add this entry at the top of the `## Changelog` list (match the recent colon style; no em dash):
```markdown
- **2026-06-16**: Web UI Settings & Printers screen (`/settings`): a key/value settings editor over
  `GET /api/settings` + `PUT /api/settings/{key}` (with `qr_base_url` suggested), and a printers CRUD
  table over `/api/printers` (ADR-0015, #23). No API change.
```

- [ ] **Step 4: Commit the docs**

```bash
git add docs/adr/0015-settings-printers-ux.md docs/adr/README.md docs/SPEC.md
git commit -m "docs: ADR-0015 settings & printers screen; SPEC changelog"
```

---

### Task 6: Adversarial review loop and integrate

The per-task commits are local WIP on `m5-settings-printers`; nothing reaches `main` until this review passes. (CLAUDE.md caps codex at 5 passes absent critical issues; this task uses the in-repo adversarial reviewer agent for the diff review.)

- [ ] **Step 1: Adversarial review of the whole diff**

Dispatch an adversarial code reviewer against the full branch diff (`git diff main...m5-settings-printers`). It audits against #23's acceptance criteria ("add/edit/remove printers; set QR base URL; settings persist"), correctness, the request shapes (`PUT /settings/{key}` body `{value}`; printer `POST`/`PUT`/`DELETE` shapes and the `204` delete; cups config `{uri}`), edge cases, the tests, and this repo's conventions. Require file:line evidence.

- [ ] **Step 2: Fix every meaningful finding, then re-review**

Address each finding (fix it, or justify with evidence). Re-dispatch the reviewer until a pass surfaces no meaningful fixes (consciously declined nits do not count).

- [ ] **Step 3: Mark P1-55 DONE with the implementation commit hash**

Capture the page implementation commit's short hash (the `feat(ui): compose Settings & Printers page` commit carrying `Fixes #23`; review fixes from this task land as their own commits, and like every other entry in the file P1-55 cites the feature's single primary implementation commit) with one exact command:
```bash
git log --grep='compose Settings & Printers page' --format=%h -n 1
```
Then in `docs/PLAN-phase-1.md` change:
```markdown
#### P1-55 Settings + printers screen · GH #23
```
to (matching the existing `· DONE (hash)` style; no em dash):
```markdown
#### P1-55 Settings + printers screen · GH #23 · DONE (<page-commit-hash>)
```
Then commit:
```bash
git add docs/PLAN-phase-1.md
git commit -m "docs: mark P1-55 (settings & printers screen) done"
```

- [ ] **Step 4: Final gate and integrate**

```bash
cd ui && npm run lint && npm run test && npm run build && cd ..
cargo fmt && cargo clippy --all-targets --all-features && cargo test
git checkout main && git merge m5-settings-printers && git push
```
No Rust changed, so the cargo gate passes unchanged but is run per the repo rule. The page commit carries `Fixes #23`, so the issue closes on push.

---

## Self-Review

**1. Spec coverage (§5):**
- Key/value editor over `GET /api/settings` + `PUT /api/settings/{key}` → Task 2 (`SettingsSection`, `SettingRow`, `useSettings`/`useUpsertSetting`).
- `qr_base_url` suggested when absent → Task 2 (`SUGGESTED_KEYS`, `suggested` flag).
- Printers add/edit/delete over `/api/printers` CRUD → Task 3 (`PrintersSection`, `PrinterForm`, `PrinterRow`, `useSavePrinter`/`useDeletePrinter`).
- kind + config (cups uri) + enabled toggle → Task 3 (`PrinterForm` fields; `config: { uri }`).
- One page, two sections → Task 4 (`Settings` composes both).
- 204 DELETE handled → Task 1 (`del` helper, no body parse).

**2. Placeholder scan:** No TBD/TODO; every code step has complete code; commands have expected output and test counts.

**3. Type consistency:** `Printer` shape (`{ id, name, kind, config, enabled }`) is consistent across Tasks 1/3 and matches `ui/src/api/types.ts`. Hook names/signatures (`useSettings`, `useUpsertSetting({key,value})`, `useSavePrinter({printer,isNew})`, `useDeletePrinter(id)`) defined in Task 1 are used identically in Tasks 2/3. `del`/`sendJson`/`getJson` match `client.ts`. The cups config accessor (`cupsUri` reading `config as { uri?: string }`) is the only place that narrows the opaque `config`.

**Known follow-up (not in this plan):** a second printer driver kind (enable the kind select + per-kind config sub-form); per-setting delete if the backend ever adds it.
