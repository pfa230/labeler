# M5 Render & Print Screen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Render & Print screen (`/print`): pick a template, fill an auto-generated field form (+ options, printer, sheet start-slot), see a debounced live preview, and Print (to a printer) or Download.

**Architecture:** Reuses the Templates foundation (`referencedFields`/`imageFields`/`defaultOptions`, the API client). Adds a `usePrinters` query, a debounced/cancellable/cached `useLivePreview` hook (single → `/api/render/label`, sheet → `/api/batch` download), and a field-form component. Print posts a batch of one to `/api/batch` `mode=print`; Download uses `/api/render/label` (single) or `/api/batch` download (sheet).

**Tech Stack:** React 19 + TS, react-router-dom 7, TanStack Query 5, Vitest + RTL.

**Spec:** `docs/superpowers/specs/2026-06-15-m5-web-ui-design.md` §Page 3 (Render & Print). M5 issue #20. Builds on the merged Templates screens + foundation.

Work on a branch:

```bash
git checkout -b m5-render-print
```

**Pre-work (CLAUDE.md):** before implementing, run at least one web search to confirm current usage for
the moving pieces here, react-router-dom 7 `useLocation`/`useNavigate`/`Routes`, TanStack Query 5
`useQuery`/`useMutation`, Vitest 4 + React Testing Library `renderHook`/`waitFor`, and the
`AbortController`/`fetch` signal pattern, since these APIs shift between versions. Note findings in the
first task's report.

---

## Backend contract facts (verified against src/)
- `POST /api/render/label` body is a flattened `LabelInput`: `{ template, data, option? }`, `?format=png|pdf` (default png), returns raw image/pdf bytes or a JSON error. `option` MUST be omitted for templates with no declared options (backend rejects it).
- `POST /api/batch` body `{ template, labels: [{ data, option? }], mode, printer?, format?, start_slot? }`. Print mode → `200 BatchSummary { total, succeeded, failed: [{ index, error }], jobs }` (served as `application/json`); print rejects `format`. Bad data → `422 BatchInvalid` with `details.failures: [{ index, code, message }]`. `start_slot` defaults to 0; the backend rejects only `start_slot > 0` for single templates (the client omits it for single regardless).
- `GET /api/printers` → `Printer[]` where `Printer = { id, name, kind, config, enabled }`.
- `start_slot` defaults to `0`; the backend rejects only `start_slot > 0` for single templates. The client omits `start_slot` for single templates regardless.

### Process note
Per-task `git commit`s are **local WIP on the `m5-render-print` branch and are never pushed**. The issue
is "ready" only after Task 5's full gate (`npm run lint`/`vitest`/`build` + `cargo fmt`/`clippy`/`test`)
and the adversarial-review loop pass; only then does the branch merge to `main`. This is the
subagent-driven-development flow and satisfies CLAUDE.md's "verified, reviewed work integrates to `main`"
(the per-task commits are not the integration point, the reviewed merge is). Mark the `P1-53` entry DONE
in the Task 5 docs commit using that commit's own hash (the last feature-branch commit), not the merge
commit's.
- Foundation already present: `referencedFields(layout, selected)`, `imageFields(layout, selected)`, `defaultOptions(options)` (`ui/src/lib/templateFields.ts`); `fetchBlob`, `submitBatch`, `saveBlob`, `ApiError` (`ui/src/api/client.ts`); `useTemplate`, `useTemplates` (`ui/src/api/queries.ts`); types `TemplateDetail`, `TemplateFormat`, `Options` (`ui/src/api/types.ts`).

---

## File map
- `ui/src/api/types.ts` — add `Printer`.
- `ui/src/api/queries.ts` — add `usePrinters`.
- `ui/src/lib/livePreview.ts` — `useLivePreview` (debounced, abortable, cached) + `previewKey`. NEW.
- `ui/src/lib/livePreview.test.ts` — NEW.
- `ui/src/pages/print/FieldForm.tsx` — auto-generated field/options/printer/start-slot form; exports the `FormValue` **type**. NEW.
- `ui/src/pages/print/FieldForm.test.tsx` — NEW.
- `ui/src/pages/print/PrintForm.tsx` — per-template form+preview+actions (keyed by templateId). NEW.
- `ui/src/pages/Print.tsx` — replace stub: template picker + keyed `PrintForm`.
- `ui/src/pages/Print.test.tsx` — NEW.

---

## Task 1: Printer type + `usePrinters`

**Files:** `ui/src/api/types.ts`, `ui/src/api/queries.ts`

- [ ] **Step 1: Type + query**

Append to `ui/src/api/types.ts`:

```ts
export interface Printer { id: string; name: string; kind: string; config: unknown; enabled: boolean }
```

Add to `ui/src/api/queries.ts`:

```ts
import type { Printer } from "./types"; // add to the existing type import
export function usePrinters() {
  return useQuery({ queryKey: ["printers"], queryFn: () => getJson<Printer[]>("/printers") });
}
```

- [ ] **Step 2: Verify + commit**

`cd ui && npm run build` clean.

```bash
git add ui/src/api
git commit -m "Add Printer type + usePrinters query (#20)"
```

---

## Task 2: `useLivePreview` hook (debounced, abortable, cached)

**Files:** `ui/src/lib/livePreview.ts`, `ui/src/lib/livePreview.test.ts`

Renders a preview object URL for the CURRENT form state. Debounced (~300ms), an `AbortController` per request so a stale render can't overwrite a newer one, and a per-request-hash cache so unchanged inputs don't re-render. Single → `/api/render/label`; sheet → `/api/batch` `mode=download`.

- [ ] **Step 1: Failing tests (pure key + hook behavior)**

`ui/src/lib/livePreview.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { previewKey, useLivePreview, type PreviewInput } from "./livePreview";

const base: PreviewInput = { templateId: "t", format: "single", data: { x: "1" }, option: { o: "a" }, startSlot: 0 };

describe("previewKey", () => {
  it("is stable regardless of key insertion order, differs on data change", () => {
    const a = previewKey({ ...base, data: { x: "1", y: "2" } });
    const b = previewKey({ ...base, data: { y: "2", x: "1" } }); // reordered
    const c = previewKey({ ...base, data: { x: "9", y: "2" } });
    expect(a).toBe(b);
    expect(a).not.toBe(c);
  });
  it("omits an empty option object from the key", () => {
    expect(previewKey({ ...base, option: {} })).toBe(previewKey({ ...base, option: undefined }));
  });
});

describe("useLivePreview", () => {
  beforeEach(() => vi.stubGlobal("fetch", vi.fn(async () => new Response(new Blob(["x"]), { status: 200 }))));
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); }); // restore URL spies too

  it("does not fetch and reports not-loading when disabled", () => {
    const { result } = renderHook(() => useLivePreview(base, false, 0));
    expect(result.current.loading).toBe(false);
    expect(fetch).not.toHaveBeenCalled();
  });

  it("fetches once after the debounce and returns a url", async () => {
    const { result } = renderHook(() => useLivePreview(base, true, 0));
    await waitFor(() => expect(result.current.url).toBeDefined());
    expect(fetch).toHaveBeenCalledTimes(1);
    expect((fetch as ReturnType<typeof vi.fn>).mock.calls[0][0]).toBe("/api/render/label");
  });

  it("reuses the cache on re-render with the same input (no second fetch)", async () => {
    const { result, rerender } = renderHook((p: { i: PreviewInput }) => useLivePreview(p.i, true, 0), {
      initialProps: { i: base },
    });
    await waitFor(() => expect(result.current.url).toBeDefined());
    rerender({ i: { ...base } }); // equal key
    await waitFor(() => expect(result.current.url).toBeDefined());
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("posts /api/batch for a sheet and omits an empty option from the body", async () => {
    const sheet: PreviewInput = { templateId: "s", format: "sheet", data: { x: "1" }, option: {} };
    const { result } = renderHook(() => useLivePreview(sheet, true, 0));
    await waitFor(() => expect(result.current.url).toBeDefined());
    const [url, init] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(url).toBe("/api/batch");
    const body = JSON.parse((init as RequestInit).body as string);
    expect(body.labels[0].option).toBeUndefined(); // empty option omitted
    expect(body.mode).toBe("download");
  });

  it("revokes cached object URLs on unmount", async () => {
    const revoke = vi.spyOn(URL, "revokeObjectURL");
    const { result, unmount } = renderHook(() => useLivePreview(base, true, 0));
    await waitFor(() => expect(result.current.url).toBeDefined());
    unmount();
    expect(revoke).toHaveBeenCalled();
  });

  it("aborts the in-flight request on key change and does not let the stale response win", async () => {
    // First call HANGS until its AbortSignal fires (rejecting AbortError); second resolves immediately.
    let call = 0;
    vi.stubGlobal("fetch", vi.fn((_url: string, init: RequestInit) => {
      call += 1;
      if (call === 1) {
        return new Promise((_resolve, reject) => {
          init.signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
        });
      }
      return Promise.resolve(new Response(new Blob(["second"]), { status: 200 }));
    }));
    const { result, rerender } = renderHook((p: { i: PreviewInput }) => useLivePreview(p.i, true, 0), { initialProps: { i: base } });
    await waitFor(() => expect(call).toBe(1));            // ensure the first (hanging) fetch has STARTED
    rerender({ i: { ...base, data: { x: "2" } } });       // new key: cleanup aborts the first request
    await waitFor(() => expect(result.current.url).toBeDefined());
    expect(result.current.error).toBeUndefined();          // the aborted first request never sets an error or stale url
    expect(call).toBe(2);
  });
});
```

(Tests that spy on `URL.createObjectURL`/`revokeObjectURL` should `vi.restoreAllMocks()` in `afterEach`
to avoid cross-test residue.)

Run: `cd ui && npx vitest run src/lib/livePreview.test.ts` → FAIL.

- [ ] **Step 2: Implement**

`ui/src/lib/livePreview.ts` (no synchronous `setState` in an effect, the disabled/cached states are
DERIVED during render; `setState` only happens inside the async timer callback, which the
`react-hooks/set-state-in-effect` rule allows):

```ts
import { useEffect, useRef, useState } from "react";

export interface PreviewInput {
  templateId: string;
  format: "single" | "sheet";   // the template's format type
  data: Record<string, string>;
  option?: Record<string, string>;
  startSlot?: number;
}

function hasOpt(o?: Record<string, string>): o is Record<string, string> {
  return !!o && Object.keys(o).length > 0;
}
const sortObj = (o?: Record<string, string>) =>
  o ? Object.fromEntries(Object.entries(o).sort(([a], [b]) => a.localeCompare(b))) : null;

export function previewKey(i: PreviewInput): string {
  return JSON.stringify([i.templateId, i.format, sortObj(i.data), hasOpt(i.option) ? sortObj(i.option) : null, i.startSlot ?? 0]);
}

interface PreviewState { url?: string; error?: string; loading: boolean }
const CACHE_MAX = 12;

// Debounced, abortable, capped-cache live preview. `enabled` gates rendering (required fields present).
// Render output is derived from STATE + the `enabled` PARAM only — the ref-backed cache is read solely
// inside the effect (the repo's `react-hooks/refs` forbids reading refs during render), and every
// `setState` happens inside the async timer (so `react-hooks/set-state-in-effect` does not fire).
export function useLivePreview(input: PreviewInput, enabled: boolean, debounceMs = 300): PreviewState {
  const key = previewKey(input);
  const cache = useRef<Map<string, string>>(new Map()); // key -> object URL (FIFO-capped)
  const [st, setSt] = useState<{ key: string; url?: string; error?: string; loading: boolean }>({ key: "", loading: false });

  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    const cached = cache.current.get(key); // ref read in the EFFECT (allowed), not during render
    const timer = setTimeout(async () => {
      if (cached) { setSt({ key, url: cached, loading: false }); return; }
      setSt({ key, loading: true });
      try {
        const single = input.format === "single";
        const path = single ? "/api/render/label" : "/api/batch";
        const label = { data: input.data, ...(hasOpt(input.option) ? { option: input.option } : {}) };
        const body = single
          ? { template: input.templateId, data: input.data, ...(hasOpt(input.option) ? { option: input.option } : {}) }
          : { template: input.templateId, mode: "download", labels: [label],
              ...(input.startSlot ? { start_slot: input.startSlot } : {}) };
        const res = await fetch(path, {
          method: "POST", headers: { "content-type": "application/json" },
          body: JSON.stringify(body), signal: controller.signal,
        });
        if (!res.ok) {
          const err = await res.json().catch(() => null);
          throw new Error(err?.error?.message ?? `preview failed (${res.status})`);
        }
        const blob = await res.blob();
        if (controller.signal.aborted) return; // unmounted/key-changed during await: drop
        const url = URL.createObjectURL(blob);
        if (cache.current.size >= CACHE_MAX) {
          const oldest = cache.current.keys().next().value as string | undefined;
          if (oldest) { URL.revokeObjectURL(cache.current.get(oldest)!); cache.current.delete(oldest); }
        }
        cache.current.set(key, url);
        setSt({ key, url, loading: false });
      } catch (e) {
        if (controller.signal.aborted || (e as Error).name === "AbortError") return; // stale: drop
        setSt({ key, error: e instanceof Error ? e.message : "preview failed", loading: false });
      }
    }, cached ? 0 : debounceMs);
    return () => { clearTimeout(timer); controller.abort(); };
  }, [key, enabled]); // eslint-disable-line react-hooks/exhaustive-deps -- input captured via `key`; debounceMs treated as constant

  useEffect(() => { const m = cache.current; return () => { for (const u of m.values()) URL.revokeObjectURL(u); }; }, []);

  // Render from state + the enabled param only (NO ref access here):
  if (!enabled) return { loading: false };
  if (st.key === key) return { url: st.url, error: st.error, loading: st.loading };
  return { loading: true }; // a newer key: the effect is debouncing/in flight
}
```

- [ ] **Step 3: Run + commit**

`cd ui && npx vitest run src/lib/livePreview.test.ts` → PASS. `npm run lint` clean (only the one
`exhaustive-deps` disable; `set-state-in-effect` does NOT fire because all `setState` is inside the async
timer callback). `npm run build` clean.

```bash
git add ui/src/lib/livePreview.ts ui/src/lib/livePreview.test.ts
git commit -m "Debounced abortable cached live-preview hook (#20)"
```

---

## Task 3: Field-form component

**Files:** `ui/src/pages/print/FieldForm.tsx`, `ui/src/pages/print/FieldForm.test.tsx`

Renders the inputs for a chosen template: one input per referenced field (text; **image fields** as a file input that reads to a data URI), a `<select>` per declared option, a printer `<select>`, and a sheet-only start-slot number input. Controlled via props; the parent owns state.

- [ ] **Step 1: Failing test**

`FieldForm.test.tsx`: render with a single-format `detail` (options `{ variant: ["a","b"] }`, referenced fields `["message"]`), a `value` initialized with the defaults (`{ data: {}, option: { variant: "a" }, printer: undefined, startSlot: 0 }` — NOT an empty option map, since the real parent passes `defaultOptions(detail.options)` and `referencedFields` treats `{}` as "render all branches"), and an `onChange` spy. Assert: a text input labelled `message` renders; a `variant` select with options a/b defaulting to `a`; typing fires `onChange` with the field value; no start-slot input for single. Then a sheet detail → a start-slot number input appears. Wrap in providers as needed (it uses `usePrinters` → QueryClientProvider + stub fetch returning `[]`).

- [ ] **Step 2: Implement**

`FieldForm.tsx` props: `{ detail: TemplateDetail; value: FormValue; onChange: (v: FormValue) => void }` and
`export type FormValue = { data: Record<string,string>; option: Record<string,string>; printer?: string; startSlot: number }`
(export it as a **type** only — a runtime non-component export from a `.tsx` trips
`react-refresh/only-export-components`).
Behavior:
- Compute `fields = referencedFields(detail.layout, value.option)` and `imgs = new Set(imageFields(detail.layout, value.option))` (re-derived as options change, so the form updates when an option gates different fields).
- For each field: if `imgs.has(field)` render an `<input type="file" accept="image/*">` ONLY (no text input) that on change reads the file via `FileReader.readAsDataURL` and stores the resulting `data:...;base64,...` URI in `value.data[field]` (show the filename once set); otherwise render an `<input type="text">` (label = field name) writing `value.data[field]`. (Image fields must be a data URI for the backend, so never offer a free-text input for them.)
- For each declared option (`detail.options`): a `<select>` of its allowed values, default the first; writing `value.option[name]`.
- A printer `<select>` from `usePrinters()` (enabled printers; show name, value = id; include an empty "— none (download only) —" choice); writes `value.printer`.
- If `detail.format.type === "sheet"`: a `<input type="number" min=0>` for `startSlot`, **coerced to a finite integer and clamped** to `[0, positions.length - 1]` (`Math.max(0, Math.min(positions.length - 1, Math.floor(Number(raw) || 0)))`), so a `NaN`/out-of-range value never reaches `PreviewInput` or the request; hidden for single.
- Mark required (all referenced fields) and surface emptiness (e.g. `aria-invalid`); the parent uses this for action gating.

- [ ] **Step 3: Run + commit**

`cd ui && npx vitest run src/pages/print/FieldForm.test.tsx` → PASS. `npm run lint && npm run build` clean.

```bash
git add ui/src/pages/print
git commit -m "Auto-generated field/options/printer/start-slot form (#20)"
```

---

## Task 4: Render & Print screen

**Files:** `ui/src/pages/Print.tsx`, `ui/src/pages/Print.test.tsx`

Two-pane. Left: a template `<select>` (from `useTemplates`), the `FieldForm`, and the actions. Right: the `useLivePreview` pane (img for single, `<object>`/`<iframe>` for sheet pdf) + **Print** / **Download** buttons.

- [ ] **Step 1: Failing test**

`Print.test.tsx` (QueryClientProvider retry:false + ToastProvider + MemoryRouter): stub `fetch` routing on the path **prefix** (e.g. `url.startsWith("/api/render/label")`) so the download's `?format=...` query still matches the same stub. **Order the branches detail-before-list**: match `/api/templates/t1` BEFORE a broad `/api/templates` branch, or the list branch will swallow the detail request. Cover `/api/templates` (list with one single template `t1`, no options), **`/api/templates/t1`** (the `TemplateDetail` with a `text` layout item `name: "message"`, `format: { type: "single", ... }` — `useTemplate` fetches this), `/api/printers` (one enabled printer), `/api/render/label` (image blob, for preview + download), `/api/batch` (a `BatchSummary` JSON with **`content-type: application/json`** — `submitBatch` decides summary-vs-download by content-type, so this header is required for the print path). The screen does NOT auto-select, so the test **selects `t1` in the template picker** (`fireEvent.change`), then: the `message` input renders once the detail loads; actions are DISABLED while `message` is empty; filling `message` enables **Download**, but **Print stays disabled until a printer is selected** (the backend requires a printer for `mode=print`), so the test must select the printer in the `FieldForm` printer `<select>` before asserting Print is enabled/clickable. Because the **live preview also** calls `/api/render/label` + `URL.createObjectURL`, assert on the **delta**: after the preview settles, record `fetch` call count, click **Download**, then assert exactly one additional `/api/render/label` POST (inspect the last call) and one additional `saveBlob` side effect (spy `URL.createObjectURL` count delta, or the anchor `click`). Clicking **Print** posts `/api/batch` with `mode: "print"` (assert the last `/api/batch` call's body) and shows the success-summary toast. (Alternatively render with `MemoryRouter initialEntries={[{ pathname: "/print", state: { template: "t1" } }]}` to preselect via the "Use to print" handoff and skip the manual select.)

- [ ] **Step 2: Implement**

Split into a parent (`Print.tsx`) that selects the template and a child (`pages/print/PrintForm.tsx`)
keyed by `templateId`, so per-template state initializes in a `useState` initializer from the LOADED
detail. This avoids any `setState`-in-effect (template/form reset comes from the remount, not an effect)
and removes the async-default-option race.

`Print.tsx` (parent):
- Call `useLocation()` at the TOP of the component, then init state from it: `const location = useLocation(); const [templateId, setTemplateId] = useState<string>(() => (location.state as { template?: string } | null)?.template ?? "")`. (Do NOT call `useLocation()` inside the `useState` initializer.)
- `useTemplates()` for the picker; a `<select>` whose `onChange` calls `setTemplateId(e.target.value)`.
- **Call `useTemplate(templateId)` UNCONDITIONALLY** (Rules of Hooks). it has `enabled: !!id`, so an empty
  id stays idle. THEN branch the render: if `templateId === ""` show a "Choose a template to start" empty
  state; else if loading show a spinner; else if `t.data` render `<PrintForm key={templateId} detail={t.data} />`.
  The `key` remounts `PrintForm` fresh on every template change. no effect, no manual reset.

`pages/print/PrintForm.tsx` (child), props `{ detail: TemplateDetail }`:
- `const [value, setValue] = useState<FormValue>(() => ({ data: {}, option: defaultOptions(detail.options), printer: undefined, startSlot: 0 }))` — synchronous init from the loaded detail (default options guaranteed before any extraction; no race).
- Render `<FieldForm detail={detail} value={value} onChange={setValue} />`.
- `const fields = referencedFields(detail.layout, value.option)`; `const valid = fields.every(f => (value.data[f] ?? "").length > 0)`.
- `const hasOptions = !!detail.options && Object.keys(detail.options).length > 0`; `const option = hasOptions ? value.option : undefined`; `const startSlot = detail.format.type === "sheet" ? value.startSlot : undefined`.
- Preview: `useLivePreview({ templateId: detail.id, format: detail.format.type, data: value.data, option, startSlot }, valid)`. Render the url (img for single, `<object>`/`<iframe>` for sheet pdf), loading + error.
- **Download** (secondary): single → `fetchBlob(\`/render/label?format=${fmt}\`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ template: detail.id, data: value.data, ...(option ? { option } : {}) }) })` then `saveBlob(blob, \`${detail.id}.${fmt}\`)`; sheet → `submitBatch({ template: detail.id, labels: [{ data: value.data, ...(option ? { option } : {}) }], mode: "download", ...(startSlot ? { start_slot: startSlot } : {}) })` → if `kind === "download"` `saveBlob(r.blob, r.filename ?? \`${detail.id}.pdf\`)`. A png/pdf toggle (`fmt`) for single download only.
- **Print** (primary, disabled if `!value.printer` or `!valid`): `submitBatch({ template: detail.id, labels: [{ data: value.data, ...(option ? { option } : {}) }], mode: "print", printer: value.printer, ...(startSlot ? { start_slot: startSlot } : {}) })`; on `kind === "summary"` toast `${succeeded}/${total}` (if `failed.length`, include `failed[0].error`); on a thrown `ApiError` with `code === "BatchInvalid"`, narrow `err.details as { failures?: { index: number; code: string; message: string }[] }` and show its messages as a **form-level** error + toast. (The backend's `failures` carry no field name, so this is form/label-level, not per-field; client-side `valid` gating already prevents most missing-field cases.)
- **`body` is always `JSON.stringify(...)`** (never a raw object) for `fetch`/`fetchBlob`. `option` omitted for no-option templates; `start_slot` omitted for single and when 0.
- **Error scope:** only `/api/batch` returns `BatchInvalid` (`details.failures`). The single **Download** path (`/render/label`) returns plain codes (`MissingField`, `InvalidOptionValue`, …), so handle those as a generic `ApiError` (toast `err.message`); the `BatchInvalid` form-level mapping applies to the **Print** (and sheet) path.
- Inline field errors (from local required-field validation) + toasts for backend errors.

- [ ] **Step 3: Run + commit**

`cd ui && npx vitest run src/pages/Print.test.tsx` → PASS. `npm run lint && npm run build` clean.

```bash
git add ui/src/pages/Print.tsx ui/src/pages/Print.test.tsx
git commit -m "Render & Print two-pane screen: form, live preview, print/download (#20)"
```

---

## Task 5: Gate + smoke + review + merge

- [ ] **Step 1: Gate**

```bash
cd ui && npm run lint && npx vitest run && npm run build && cd ..
cargo fmt --check && cargo clippy --all-targets --all-features && cargo test   # full backend gate, no pipe to mask failures
```
All green (frontend lint/test/build; backend fmt/clippy/test, unchanged but run per the repo gate).

- [ ] **Step 2: Manual smoke**

`npm --prefix ui run build && cargo run`, open `/print`: pick `brother24mm`, fill `message`/`code`, preview updates (debounced); Download writes a PNG; with a configured printer, Print shows a summary. From a template detail page, "Use to print" lands on `/print` with that template preselected. Stop the server.

- [ ] **Step 3: Docs**

Per CLAUDE.md a behavior change updates `docs/SPEC.md` AND adds/supersedes the relevant ADR. So:
- **`docs/SPEC.md`:** add a changelog line under the web-UI note: "Web UI: Render & Print screen
  (`/print`), pick a template, fill fields/options, live preview, print or download (#20)." (API tables
  unchanged.)
- **ADR-0013** (new, short): "Render & Print UX decisions", recording the choices this screen fixes that
  ADR-0008 did not: single Download uses `/render/label` (raw file) rather than a `/batch` ZIP-of-one;
  print posts a batch-of-one to `/batch`; the form auto-generates from option-aware referenced fields;
  image fields are entered as data URIs; the live preview is debounced/abortable. Status Accepted;
  references ADR-0008. Add its row to `docs/adr/README.md`.
- **`docs/PLAN-phase-1.md`:** mark the `P1-53` entry **DONE**.

A commit can't contain its own hash, so commit then amend in the hash:

```bash
git add docs/SPEC.md docs/adr/0013-render-print-ux.md docs/adr/README.md docs/PLAN-phase-1.md
git commit -m "Render & Print: SPEC changelog + ADR-0013 + mark P1-53 done (#20)"
# edit the P1-53 DONE line to include $(git rev-parse --short HEAD), then:
git add docs/PLAN-phase-1.md && git commit --amend --no-edit
```

- [ ] **Step 4: Adversarial review + merge**

Reviewer → fix loop on `git diff main...m5-render-print` (focus: `option`/`start_slot` omission rules, the abort/stale-preview guard + object-URL cache cap/cleanup, image-field file→data-URI, action gating on validity + printer presence, `422 BatchInvalid` → form-level mapping, the keyed-`PrintForm` reset behavior, a11y of the form). Then:

```bash
git checkout main && git merge --no-ff m5-render-print -m "Merge M5 Render & Print screen

Fixes #20" && git push
```
`#20` ("Render / print form") IS completed by this work, so `Fixes #20` closes it on push. (`P1-53` was
already marked DONE in the Step 3 docs commit. CSV `#24` and Settings `#23` are separate issues/plans.)

---

## Self-review notes
- **Spec coverage:** template picker + auto-form from referenced fields (T3,T4), options/printer/start-slot (T3), debounced cancellable cached preview (T2), single vs sheet preview + download + print to `/batch` (T4), inline + toast errors incl. `BatchInvalid` (T4), "Use to print" prefill (T4). CSV grid and Settings are separate plans.
- **Type consistency:** `PreviewInput`/`previewKey`/`useLivePreview` (T2) used by Print (T4); `FormValue` shape shared by `FieldForm` (T3) and `Print` (T4); `referencedFields`/`imageFields`/`defaultOptions` reused from the foundation; `usePrinters`/`Printer` (T1) used by `FieldForm`/`Print`.
- **Verify at impl:** the `react-hooks/set-state-in-effect` + `exhaustive-deps` rules (the foundation enforces them, see how `preview.ts` resolved set-state-in-effect by doing the `setState` inside the async body; apply the same pattern in `useLivePreview` if the linter flags the `setState({loading:true})`); the sheet-PDF preview embed; that `fetchBlob` accepts a `RequestInit` for the download POST body.
