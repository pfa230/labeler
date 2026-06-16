# M5 Templates Screens Implementation Plan (list + detail + shared foundation)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Templates list and detail screens, and the shared frontend foundation they establish, the expanded `/api` types/client, the option-aware referenced-field/settings extraction util, and the preview hook, which Render & Print and the CSV grid reuse.

**Architecture:** Pure additions in `ui/`. Read-only data from `GET /api/templates`, `/api/templates/{id}`, `/api/templates/{id}/source`; create via `POST /api/templates`. Field/settings extraction walks the template `layout` (option-aware). Detail preview synthesizes sample field values + real settings and renders via `/api/render/label` (single) or `/api/batch` download (sheet).

**Tech Stack:** React 19 + TS, react-router-dom 7, TanStack Query 5, Vitest + RTL.

**Spec:** `docs/superpowers/specs/2026-06-15-m5-web-ui-design.md` §Pages 1-2, §"Reusable…", §field extraction. M5 issue #17. This is the first of the screens plans; Render & Print, the CSV grid, and Settings are subsequent plans.

Work on a branch:

```bash
git checkout -b m5-templates-screens
```

---

## File map
- `ui/src/api/types.ts` — extend with `TemplateDetail`, `LayoutItem`, `Options`, format types.
- `ui/src/api/client.ts` — (already has getJson/sendJson/fetchBlob/submitBatch) used as-is.
- `ui/src/api/queries.ts` — add `useTemplate`, `useTemplateSource`, `useCreateTemplate`.
- `ui/src/lib/templateFields.ts` — option-aware `referencedFields` + `referencedSettings` + `defaultOptions`. NEW.
- `ui/src/lib/templateFields.test.ts` — tests. NEW.
- `ui/src/lib/preview.ts` — `useTemplatePreview(detail)` hook + `sampleData`. NEW.
- `ui/src/pages/Templates.tsx` — replace placeholder with the list screen.
- `ui/src/pages/TemplateDetail.tsx` — NEW detail screen.
- `ui/src/pages/NewTemplate.tsx` — NEW raw-YAML create view.
- `ui/src/app/App.tsx` — add `/templates/:id` and `/templates/new` routes.
- `ui/src/pages/*.test.tsx` — screen tests.

---

## Task 1: API types + queries

**Files:** `ui/src/api/types.ts`, `ui/src/api/queries.ts`

- [ ] **Step 1: Extend types**

Append to `ui/src/api/types.ts` (the `layout` is a JSON array of tagged items; `format` is tagged by `type`):

```ts
export type Dimension = number | { min?: number; max?: number };
export type TemplateFormat =
  | { type: "single"; width: Dimension; height: Dimension }
  | { type: "sheet"; paper_width: number; paper_height: number; label_width: number; label_height: number; positions: [number, number][] };

export type Options = Record<string, string[]>;

// Layout items are tagged by `type`; only the fields the UI reads are typed.
export type LayoutItem =
  | { type: "text"; name?: string; value?: string }
  | { type: "qr"; name?: string; value?: string }
  | { type: "image"; name?: string; src?: string }
  | { type: "line" }
  | { type: "container"; option?: Record<string, string>; items: LayoutItem[] };

export interface TemplateDetail {
  id: string; name: string; description: string; unit: string; dpi: number;
  format: TemplateFormat; options?: Options; layout: LayoutItem[]; version?: string;
}
```

Also widen `TemplateSummary.format` to `TemplateFormat["type"]`-bearing: change it to `format: { type: "single" | "sheet" }` and add `options?: Options`.

- [ ] **Step 2: Add queries**

In `ui/src/api/queries.ts`:

```ts
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getJson, sendJson } from "./client";
import type { TemplateSummary, TemplateDetail } from "./types";

export function useTemplates() {
  return useQuery({ queryKey: ["templates"], queryFn: () => getJson<{ templates: TemplateSummary[] }>("/templates") });
}
export function useTemplate(id: string) {
  return useQuery({ queryKey: ["template", id], queryFn: () => getJson<TemplateDetail>(`/templates/${id}`), enabled: !!id });
}
export function useTemplateSource(id: string) {
  return useQuery({
    queryKey: ["template-source", id],
    queryFn: async () => {
      const res = await fetch(`/api/templates/${id}/source`);
      if (!res.ok) throw new Error(`source ${res.status}`);
      return res.text();
    },
    enabled: !!id,
  });
}
export function useCreateTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (yaml: string) => {
      const res = await fetch("/api/templates", { method: "POST", headers: { "content-type": "text/yaml" }, body: yaml });
      if (!res.ok) {
        const body = await res.json().catch(() => null);
        throw new Error(body?.error?.message ?? `create failed (${res.status})`);
      }
      return (await res.json()) as TemplateDetail;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["templates"] }),
  });
}
```

(`/templates` create takes a raw YAML body, not JSON, hence the direct `fetch` with `text/yaml`. Keep the existing `useTemplates`.)

- [ ] **Step 3: Verify**

`cd ui && npm run build` succeeds; `npx tsc -b` clean. Commit.

```bash
git add ui/src/api
git commit -m "Add template detail/source/create types + queries (#17)"
```

---

## Task 2: Option-aware field + settings extraction

**Files:** `ui/src/lib/templateFields.ts`, `ui/src/lib/templateFields.test.ts`

- [ ] **Step 1: Write the failing tests**

`ui/src/lib/templateFields.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { referencedFields, referencedSettings, defaultOptions } from "./templateFields";
import type { LayoutItem, Options } from "../api/types";

const layout: LayoutItem[] = [
  { type: "text", name: "title" },
  { type: "qr", value: "{settings.qr_base_url}/{id}" },
  { type: "image", name: "logo" },
  { type: "text", value: "literal {{not a field}}" },
  { type: "container", option: { orientation: "horizontal" }, items: [{ type: "text", name: "h_only" }] },
  { type: "container", option: { orientation: "vertical" }, items: [{ type: "text", name: "v_only" }] },
];
const options: Options = { orientation: ["horizontal", "vertical"] };

describe("referencedFields", () => {
  it("collects name + value tokens + image.name, skips literal braces", () => {
    const f = referencedFields(layout, { orientation: "horizontal" });
    expect(f).toContain("title");
    expect(f).toContain("id");       // from {id} in the qr value
    expect(f).toContain("logo");     // image.name
    expect(f).toContain("h_only");   // matching container
    expect(f).not.toContain("v_only"); // gated out by option
    expect(f).not.toContain("not a field"); // {{ }} escape is literal
    expect(f).not.toContain("settings.qr_base_url"); // settings are not data fields
  });
  it("defaultOptions picks the first allowed value", () => {
    expect(defaultOptions(options)).toEqual({ orientation: "horizontal" });
  });
});

describe("referencedSettings", () => {
  it("collects {settings.*} keys", () => {
    expect(referencedSettings(layout)).toContain("qr_base_url");
  });
});
```

Run: `cd ui && npx vitest run src/lib/templateFields.test.ts` → FAIL (module missing).

- [ ] **Step 2: Implement**

`ui/src/lib/templateFields.ts`:

```ts
import type { LayoutItem, Options } from "../api/types";

// Parse `{field}` / `{settings.key}` tokens from an interpolation string, honoring `{{`/`}}` escapes.
function tokens(s: string): string[] {
  const out: string[] = [];
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (c === "{" && s[i + 1] === "{") { i += 2; continue; }
    if (c === "}" && s[i + 1] === "}") { i += 2; continue; }
    if (c === "{") {
      const end = s.indexOf("}", i + 1);
      if (end === -1) break;
      out.push(s.slice(i + 1, end));
      i = end + 1;
      continue;
    }
    i += 1;
  }
  return out;
}

export function defaultOptions(options?: Options): Record<string, string> {
  const sel: Record<string, string> = {};
  for (const [k, vals] of Object.entries(options ?? {})) if (vals[0] !== undefined) sel[k] = vals[0];
  return sel;
}

function walk(items: LayoutItem[], selected: Record<string, string>, onToken: (t: string) => void) {
  for (const it of items) {
    if (it.type === "text" || it.type === "qr") {
      if (it.name) onToken(it.name);
      if (it.value) for (const t of tokens(it.value)) onToken(t);
    } else if (it.type === "image") {
      if (it.name) onToken(it.name);
    } else if (it.type === "container") {
      // gate: a container with an `option` only contributes when the selection matches all its keys
      const match = Object.entries(it.option ?? {}).every(([k, v]) => selected[k] === v);
      if (match) walk(it.items, selected, onToken);
    }
  }
}

export function referencedFields(layout: LayoutItem[], selected: Record<string, string>): string[] {
  const set = new Set<string>();
  walk(layout, selected, (t) => { if (!t.startsWith("settings.")) set.add(t); });
  return [...set];
}

export function referencedSettings(layout: LayoutItem[]): string[] {
  const set = new Set<string>();
  // settings are not option-gated for discovery; collect across all branches
  const all: Record<string, string> = {}; // empty selection still matches option-less containers; for settings,
  walkAllForSettings(layout, set);
  void all;
  return [...set];
}

function walkAllForSettings(items: LayoutItem[], set: Set<string>) {
  for (const it of items) {
    if ((it.type === "text" || it.type === "qr") && it.value) {
      for (const t of tokens(it.value)) if (t.startsWith("settings.")) set.add(t.slice("settings.".length));
    } else if (it.type === "container") {
      walkAllForSettings(it.items, set);
    }
  }
}
```

- [ ] **Step 3: Run to verify it passes**

Run: `cd ui && npx vitest run src/lib/templateFields.test.ts` → PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/src/lib/templateFields.ts ui/src/lib/templateFields.test.ts
git commit -m "Option-aware referenced-field + settings extraction (#17)"
```

---

## Task 3: Preview hook (synthesized sample data)

**Files:** `ui/src/lib/preview.ts`, `ui/src/lib/preview.test.ts`

The detail preview needs values for the referenced fields. Synthesize a sample per field (the field name as a readable stand-in), fetch settings for `{settings.*}`, and render: single → `/api/render/label` blob; sheet → `/api/batch` download blob (one label). Returns an object URL + a cleanup.

- [ ] **Step 1: Test the sample-data builder (pure part)**

`ui/src/lib/preview.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { sampleData } from "./preview";

describe("sampleData", () => {
  it("builds a value per referenced field", () => {
    expect(sampleData(["title", "id"])).toEqual({ title: "title", id: "id" });
  });
});
```

Run: `cd ui && npx vitest run src/lib/preview.test.ts` → FAIL.

- [ ] **Step 2: Implement**

`ui/src/lib/preview.ts`:

```ts
import { useEffect, useState } from "react";
import { fetchBlob, submitBatch } from "../api/client";
import { defaultOptions, referencedFields } from "./templateFields";
import type { TemplateDetail } from "../api/types";

export function sampleData(fields: string[]): Record<string, string> {
  return Object.fromEntries(fields.map((f) => [f, f]));
}

// Renders a preview object URL for a template detail. Single -> /render/label image; sheet -> /batch pdf.
export function useTemplatePreview(detail: TemplateDetail | undefined): { url?: string; error?: string; loading: boolean } {
  const [state, setState] = useState<{ url?: string; error?: string; loading: boolean }>({ loading: false });
  useEffect(() => {
    if (!detail) return;
    let url: string | undefined;
    let cancelled = false;
    setState({ loading: true });
    const option = defaultOptions(detail.options);
    const data = sampleData(referencedFields(detail.layout, option));
    (async () => {
      try {
        let blob: Blob;
        if (detail.format.type === "single") {
          ({ blob } = await fetchBlob("/render/label", {
            method: "POST", headers: { "content-type": "application/json" },
            body: JSON.stringify({ template: detail.id, data, option }),
          }));
        } else {
          const r = await submitBatch({ template: detail.id, labels: [{ data, option }], mode: "download" });
          if (r.kind !== "download") throw new Error("expected a sheet PDF");
          blob = r.blob;
        }
        if (cancelled) return;
        url = URL.createObjectURL(blob);
        setState({ url, loading: false });
      } catch (e) {
        if (!cancelled) setState({ error: e instanceof Error ? e.message : "preview failed", loading: false });
      }
    })();
    return () => { cancelled = true; if (url) URL.revokeObjectURL(url); };
  }, [detail]);
  return state;
}
```

Note: `/api/render/label` and `/api/print`-style bodies take `{ template, data, option }` (flattened `LabelInput`), matching the backend `RenderLabelRequest`. Verify the field name (`option`) against `src/models.rs` `LabelInput` during impl.

- [ ] **Step 3: Run**

Run: `cd ui && npx vitest run src/lib/preview.test.ts` → PASS (the `sampleData` unit; the hook is exercised by the detail screen test with a mocked fetch).

- [ ] **Step 4: Commit**

```bash
git add ui/src/lib/preview.ts ui/src/lib/preview.test.ts
git commit -m "Template preview hook with synthesized sample data (#17)"
```

---

## Task 4: Templates list screen

**Files:** `ui/src/pages/Templates.tsx`, `ui/src/pages/Templates.test.tsx`

- [ ] **Step 1: Failing test**

`ui/src/pages/Templates.test.tsx`: render `Templates` inside `QueryClientProvider` + `MemoryRouter`, with `fetch` stubbed to return two templates (one single, one sheet). Assert both names render, the single/sheet badges show, and typing in the search box filters by id. Example assertions: `await screen.findByText("Brother 24mm")`, `screen.getByText("sheet")`, after `fireEvent.change(searchBox, { target: { value: "avery" }})` only the matching card remains.

- [ ] **Step 2: Implement**

Replace `ui/src/pages/Templates.tsx`: use `useTemplates()`; render a responsive grid of cards (id monospace chip, name, single/sheet badge, a static placeholder thumbnail per the design, no live render); a search `<input>` filtering by id (client-side); loading + empty states; on error push an error toast. Each card is a `Link` to `/templates/${id}`. A "New template" `Link` to `/templates/new`. Use theme vars for styling.

- [ ] **Step 3: Run + commit**

`cd ui && npx vitest run src/pages/Templates.test.tsx` → PASS. `npm run build` clean.

```bash
git add ui/src/pages/Templates.tsx ui/src/pages/Templates.test.tsx
git commit -m "Templates list screen: cards, search, badges (#17)"
```

---

## Task 5: Template detail screen

**Files:** `ui/src/pages/TemplateDetail.tsx`, `ui/src/pages/TemplateDetail.test.tsx`, `ui/src/app/App.tsx`

- [ ] **Step 1: Failing test**

`ui/src/pages/TemplateDetail.test.tsx`: render with a route `/templates/brother24mm` (use `MemoryRouter initialEntries` + a `Routes`), stub `fetch` for `/api/templates/brother24mm` (detail), `/api/templates/brother24mm/source` (yaml text), and the preview render (return a small blob). Assert: the name renders, the referenced field names appear (e.g. `message`, `code`), the format badge shows, the raw YAML source toggle reveals the yaml text, and a "Use to print" link/button is present pointing at `/print`.

- [ ] **Step 2: Implement**

`ui/src/pages/TemplateDetail.tsx`: read `:id` (`useParams`); `useTemplate(id)`, `useTemplateSource(id)`, `useTemplatePreview(detail)`. Render: a preview pane (`<img>` for single via the preview url; `<iframe>`/`<object>` for the sheet PDF url) with loading/error states; metadata (unit, dpi, format + dimensions, declared options); the referenced field names (`referencedFields(detail.layout, defaultOptions(detail.options))`) and settings keys (`referencedSettings`); a collapsible raw-YAML block (`<details>` or a toggle) showing the source; and a "Use to print" `Link` to `/print` passing the template id via router state (`<Link to="/print" state={{ template: id }}>`). Single vs sheet indicated.

- [ ] **Step 3: Route**

In `ui/src/app/App.tsx`, add child routes under the Shell layout: `/templates/:id` → `TemplateDetail`, `/templates/new` → `NewTemplate` (Task 6). Ensure these are matched before the catch-all redirect.

- [ ] **Step 4: Run + commit**

`cd ui && npx vitest run src/pages/TemplateDetail.test.tsx` → PASS. `npm run build` clean.

```bash
git add ui/src/pages/TemplateDetail.tsx ui/src/pages/TemplateDetail.test.tsx ui/src/app/App.tsx
git commit -m "Template detail screen: preview, metadata, fields, source, use-to-print (#17)"
```

---

## Task 6: New-template (raw YAML) view

**Files:** `ui/src/pages/NewTemplate.tsx`, `ui/src/pages/NewTemplate.test.tsx`

- [ ] **Step 1: Failing test**

`ui/src/pages/NewTemplate.test.tsx`: render in providers + router; stub `fetch` POST `/api/templates` to 201 with a detail body; type YAML into the textarea, click "Create", assert it navigates to the new detail (or shows success) and on a 422 it shows the error message from the contract.

- [ ] **Step 2: Implement**

`ui/src/pages/NewTemplate.tsx`: a `<textarea>` for raw YAML, a "Create" button using `useCreateTemplate()`; on success navigate to `/templates/${created.id}` and toast ok; on error show the `error.message` inline + toast. This is NOT the GUI editor (out of scope), just a raw-YAML create form against the existing endpoint.

- [ ] **Step 3: Run + commit**

`cd ui && npx vitest run src/pages/NewTemplate.test.tsx` → PASS.

```bash
git add ui/src/pages/NewTemplate.tsx ui/src/pages/NewTemplate.test.tsx
git commit -m "Raw-YAML new-template view (#17)"
```

---

## Task 7: Gate + smoke + review + merge

- [ ] **Step 1: Gate**

```bash
cd ui && npm run lint && npx vitest run && npm run build && cd ..
cargo test 2>&1 | tail -5   # backend unaffected; still green
```
All green: lint clean, all vitest pass, build succeeds.

- [ ] **Step 2: Manual smoke**

`npm --prefix ui run build && cargo run`, open `http://localhost:8080/`: Templates lists the starter set; click one → detail shows a rendered preview, metadata, referenced fields, raw YAML; "Use to print" navigates to /print (placeholder for now). Create a trivial template via "New template". Stop the server.

- [ ] **Step 3: Adversarial review + merge**

Run the reviewer → fix loop on `git diff main...m5-templates-screens` (focus: option-aware extraction correctness, preview object-URL cleanup/cancellation, the sheet-PDF preview embed, error/empty states, the create-view error contract, a11y of the cards/links, no test trusting only render). Then:

```bash
git checkout main && git merge m5-templates-screens && git push
```
Reference `#17`; do not close #15/#17 (Render & Print, CSV, Settings screens remain).

---

## Self-review notes
- **Spec coverage:** types/queries (T1), option-aware field + settings extraction incl. `image.name` and `{field}`/`{settings.*}` tokens (T2), preview with synthesized sample data + real settings, single via `/render/label` and sheet via `/batch` (T3), list with cards/search/badges/placeholder-thumbnails (T4), detail with preview/metadata/fields/source/use-to-print (T5), raw-YAML create (T6). Render & Print, CSV grid, and Settings are the next screens plans.
- **Type consistency:** `referencedFields(layout, selected)` / `referencedSettings(layout)` / `defaultOptions(options)` defined in T2 and used by the preview hook (T3) and the detail screen (T5); `TemplateDetail`/`LayoutItem`/`TemplateFormat`/`Options` defined in T1 and used throughout; queries `useTemplate`/`useTemplateSource`/`useCreateTemplate` (T1) used by T5/T6.
- **Verify at impl:** the `LabelInput` JSON shape (`{ template, data, option }` flattened) for the preview render body against `src/models.rs`; that the sheet-PDF preview embeds acceptably in an `<iframe>`/`<object>` (object URL); RTL async query testing needs `QueryClientProvider` with retries disabled in tests.
