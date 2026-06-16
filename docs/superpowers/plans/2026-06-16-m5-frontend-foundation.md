# M5 Frontend Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the React/TS/Vite SPA in `ui/`, serve its build from axum (SPA fallback into the `/api` app), and ship the app shell (sidebar nav, Ink & Tape theme, toasts, routing) plus the typed data layer, so the screens plan only needs to fill in pages.

**Architecture:** A Vite React-TS app under `ui/`. In dev, `vite dev` proxies `/api` to axum (hot reload). In prod, axum serves `ui/dist` (hashed assets via `ServeDir`, all non-`/api` routes fall back to `index.html`); unknown `/api/*` still returns the JSON `NotFound`. The shell uses React Router; server state goes through TanStack Query and a hand-written typed `/api` client (binary endpoints branch on content-type).

**Tech Stack:** React 18 + TypeScript, Vite, Tailwind CSS, React Router, TanStack Query, Vitest + React Testing Library; backend axum + tower-http `fs`.

**Spec:** `docs/superpowers/specs/2026-06-15-m5-web-ui-design.md` (Architecture, `/api` serving, Data layer, Shell, Visual style). This is M5 issue #15's frontend half; the backend `/api` migration already shipped. The five screens are the NEXT plan, this one ships the shell with placeholder route pages.

**Out of scope (deferred):** the five real screens; the reusable label grid; Docker packaging (M6); e2e/Playwright (screens plan).

Work on a branch:

```bash
git checkout -b m5-frontend-foundation
```

---

## File map
- `ui/` — new Vite app: `package.json`, `vite.config.ts`, `tsconfig*.json`, `index.html`, `src/`.
- `ui/src/theme.css` — Ink & Tape Tailwind layer + CSS-variable tokens (light + dark).
- `ui/src/api/` — `client.ts` (fetch wrapper + error type), `types.ts` (API types), `queries.ts` (TanStack hooks).
- `ui/src/app/` — `App.tsx` (router), `Shell.tsx` (sidebar + outlet + toast region), `ThemeToggle.tsx`, `toast.tsx`.
- `ui/src/pages/` — placeholder `Templates.tsx`, `Print.tsx`, `Import.tsx`, `Settings.tsx`.
- `src/api.rs` — replace the SPA branch of `fallback` with index.html serving; mount `ServeDir` for assets; read UI dir from env.
- `Cargo.toml` — add tower-http `fs` feature.
- `.gitignore` — `ui/node_modules`, `ui/dist`.
- `docs/SPEC.md` — note the SPA is served at `/` from `ui/dist`.

---

## Task 1: Scaffold the Vite React-TS app

**Files:** `ui/**`, `.gitignore`

- [ ] **Step 1: Scaffold**

From the repo root:

```bash
npm create vite@latest ui -- --template react-ts
cd ui && npm install
```

Expected: `ui/` contains a working Vite React-TS app; `npm install` succeeds.

- [ ] **Step 2: Add deps**

```bash
cd ui
npm install react-router-dom @tanstack/react-query
npm install -D tailwindcss@^3 postcss autoprefixer vitest @testing-library/react @testing-library/jest-dom jsdom
npx tailwindcss init -p
```

(Use Tailwind v3 for the stable PostCSS setup. If the scaffold pulls React 19, that's fine.)

- [ ] **Step 3: gitignore**

Append to the repo-root `.gitignore`:

```
ui/node_modules
ui/dist
```

- [ ] **Step 4: Tailwind config**

Set `ui/tailwind.config.js` `content` to `["./index.html", "./src/**/*.{ts,tsx}"]` and enable class dark mode:

```js
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: ["class"],
  theme: { extend: {} },
  plugins: [],
};
```

- [ ] **Step 5: Verify dev build**

```bash
cd ui && npm run build
```

Expected: a `ui/dist/` with `index.html` + `assets/`. Commit.

```bash
cd /Users/pfa/projects/labeler
git add ui .gitignore
git commit -m "Scaffold Vite React-TS app in ui/ (#15)"
```

---

## Task 2: Ink & Tape theme tokens + base styles

**Files:** `ui/src/theme.css`, `ui/src/main.tsx`

- [ ] **Step 1: Write the theme layer**

Create `ui/src/theme.css` (tokens taken from the approved mockup `docs/superpowers/specs/m5-ui-mockups.html`). Light default; `.dark` overrides:

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

:root {
  --paper: #faf8f3; --surface: #ffffff; --ink: #1c1a17; --muted: #6f6a60; --faint: #a39e92;
  --border: #e7e2d8; --accent: #e4572e; --accent-soft: #fbe9e2; --good: #3f7d3a; --bad: #c2401c;
}
.dark {
  --paper: #16140f; --surface: #1f1c16; --ink: #f2efe7; --muted: #b8b2a4; --faint: #8a8478;
  --border: #2c2820; --accent: #f0784f; --accent-soft: #3a241c; --good: #7fb074; --bad: #e6855f;
}
html, body, #root { height: 100%; }
body {
  margin: 0; background: var(--paper); color: var(--ink);
  font-family: ui-sans-serif, system-ui, -apple-system, sans-serif;
}
.mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
@media (prefers-reduced-motion: reduce) {
  * { animation-duration: 0.01ms !important; transition-duration: 0.01ms !important; }
}
```

Replace the Vite default `ui/src/index.css` import in `ui/src/main.tsx` with `import "./theme.css";` (delete `index.css` and `App.css` if unused).

- [ ] **Step 2: Pre-paint theme (no FOUC)**

Add an inline script to `ui/index.html` `<head>`, BEFORE the `<script type="module" src="/src/main.tsx">`,
so the dark class is set before first paint (a React effect runs too late and flashes). Wrap in
`try/catch` for private-mode/SSR safety:

```html
<script>
  try {
    var t = localStorage.getItem("theme");
    var dark = t ? t === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
    if (dark) document.documentElement.classList.add("dark");
  } catch (_) {}
</script>
```

The `ThemeToggle` component (Task 6) only toggles/persists after load; it must not read
`document`/`localStorage` at module scope.

- [ ] **Step 3: Verify**

`cd ui && npm run build` succeeds. Commit.

```bash
git add ui/src ui/index.html
git commit -m "Add Ink & Tape theme tokens + pre-paint theme init (#15)"
```

---

## Task 3: Backend serves the SPA

**Files:** `Cargo.toml`, `src/api.rs`, `src/lib.rs` (tests)

NOTE on versions: the repo is on **axum 0.8** (`Cargo.toml`), `tower-http 0.7` (currently `trace` only).
Verify API against those versions. The UI dir is threaded through `AppState` (NOT read from a global env
var in the handler) so parallel tests can each set their own dir without racing.

- [ ] **Step 1: Add `ui_dir` to `AppState`**

In `src/api.rs`, add a `ui_dir: PathBuf` field to `AppState`. In `AppState::new(...)`, default it to
`LABELER_UI_DIR` env or `ui/dist`; add a builder for tests:

```rust
// in struct AppState { … , ui_dir: std::path::PathBuf }
// in AppState::new(...), set:
//   ui_dir: std::env::var_os("LABELER_UI_DIR").map(Into::into)
//            .unwrap_or_else(|| std::path::PathBuf::from("ui/dist")),
pub fn with_ui_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
    self.ui_dir = dir.into();
    self
}
pub fn ui_dir(&self) -> &std::path::Path { &self.ui_dir }
```

`main.rs` keeps calling `AppState::new(...)` unchanged (it picks up the env/default).

- [ ] **Step 2: Write the failing tests**

In `src/lib.rs` `http_tests`, REPLACE the existing `root_path_is_not_the_api` test (it currently asserts
`/health` → 404, which becomes nondeterministic once a real `ui/dist` exists) and add the SPA + asset
tests. Each builds an app with an explicit, isolated `ui_dir` (no global env, no race). Add a small
helper near `build_app`:

```rust
    fn app_with_ui(dir: &std::path::Path) -> axum::Router {
        let templates = TemplateRegistry::load_from_dir("templates").expect("load templates");
        let store = Store::open_in_memory().expect("store");
        app(Arc::new(
            AppState::new(templates, "templates".into(), store).with_ui_dir(dir),
        ))
    }

    #[tokio::test]
    async fn root_path_serves_spa_not_api() {
        // empty ui dir (no index.html): the old root API path is gone; /health is not the API.
        let dir = std::env::temp_dir().join(format!("labeler_ui_empty_{}", uniq()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = app_with_ui(&dir);
        let res = app.clone()
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND); // not the API; no index.html present
        let res = app.clone()
            .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn spa_fallback_serves_index_for_non_api() {
        let dir = std::env::temp_dir().join(format!("labeler_ui_{}", uniq()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), "<!doctype html><title>labeler ui</title>").unwrap();
        let app = app_with_ui(&dir);

        // a client-side route falls back to index.html
        let res = app.clone()
            .oneshot(Request::builder().uri("/templates/abc").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap().to_string();
        assert!(ct.contains("text/html"), "got {ct}");
        let body = axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("labeler ui"));

        // unknown API path still returns the JSON contract
        let res = app.clone()
            .oneshot(Request::builder().uri("/api/nope").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "NotFound");

        // a missing asset is a 404 (NOT the SPA html) — assets must not be shadowed by index.html
        let res = app.clone()
            .oneshot(Request::builder().uri("/assets/missing.js").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let ct = res.headers().get("content-type").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
        assert!(!ct.contains("text/html"), "missing asset must not serve SPA html");

        std::fs::remove_dir_all(&dir).ok();
    }
```

Add a `uniq()` test helper if one doesn't exist (a process-id + atomic counter, or nanos timestamp) so
temp dirs don't collide across parallel tests.

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test --lib http_tests::root_path_serves_spa_not_api http_tests::spa_fallback_serves_index_for_non_api 2>&1 | tail -20`
Expected: FAIL (`with_ui_dir`/`ui_dir` don't exist yet; the non-`/api` branch still returns plain text).

- [ ] **Step 4: Add tower-http `fs`**

In `Cargo.toml`: `tower-http = { version = "0.7", features = ["trace", "fs"] }`.

- [ ] **Step 5: Serve assets + index.html from `AppState.ui_dir`**

Mount `ServeDir` for assets and make `fallback` a `State`-aware handler reading `state.ui_dir()`. Update
`app()`:

```rust
pub fn app(state: Arc<AppState>) -> Router {
    let assets = tower_http::services::ServeDir::new(state.ui_dir().join("assets"));
    Router::new()
        .nest("/api", api_router())
        .nest_service("/assets", assets)
        .fallback(fallback)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn fallback(State(state): State<Arc<AppState>>, uri: axum::http::Uri) -> Response {
    if uri.path() == "/api" || uri.path().starts_with("/api/") {
        return AppError::not_found(uri.path()).into_response();
    }
    // SPA: serve index.html for any non-API, non-asset route (client-side routing).
    match tokio::fs::read(state.ui_dir().join("index.html")).await {
        Ok(bytes) => (
            axum::http::StatusCode::OK,
            [("content-type", "text/html; charset=utf-8")],
            bytes,
        )
            .into_response(),
        Err(_) => (
            axum::http::StatusCode::NOT_FOUND,
            "UI not built; run `npm --prefix ui run build`",
        )
            .into_response(),
    }
}
```

Correctness notes (verified against axum/tower-http docs): a nested/mounted service that returns 404
does **not** fall through to the router fallback, so a **missing `/assets/*` file returns ServeDir's 404**
(good, not the SPA html). Unknown `/api/*` is caught by this `fallback` (the outer fallback applies to the
nest's unmatched paths) and returns JSON. `ServeDir` rejects `..`/backslash traversal itself.

- [ ] **Step 6: Run to verify they pass**

Run: `cargo test --lib http_tests::root_path_serves_spa_not_api http_tests::spa_fallback_serves_index_for_non_api 2>&1 | tail -20`
Expected: PASS. Then `cargo test 2>&1 | tail -5` → all pass; `cargo fmt`; `cargo clippy --all-targets --all-features` clean.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/api.rs src/lib.rs
git commit -m "Serve the SPA build from axum (AppState.ui_dir) with index.html fallback (#15)"
```

---

## Task 4: Vite dev proxy + build config

**Files:** `ui/vite.config.ts`

- [ ] **Step 1: Configure proxy + build base**

Set `ui/vite.config.ts`:

```ts
/// <reference types="vitest/config" />
import { defineConfig } from "vitest/config"; // re-exports vite's defineConfig + the `test` field
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  base: "/",
  build: { outDir: "dist" },
  server: {
    proxy: { "/api": { target: "http://localhost:8080", changeOrigin: true } },
  },
  test: { environment: "jsdom", setupFiles: "./src/setupTests.ts", globals: true },
});
```

(Importing `defineConfig` from `vitest/config` keeps the `test` field type-valid; importing it from
`vite` would make the template's typecheck reject `test` and break `npm run build`.)

Create `ui/src/setupTests.ts`:

```ts
import "@testing-library/jest-dom";
```

Add a `test` script to `ui/package.json`: `"test": "vitest run"`.

- [ ] **Step 2: Verify**

`cd ui && npm run build` succeeds; `npx vitest run` runs (0 tests yet is fine). Commit.

```bash
git add ui/vite.config.ts ui/src/setupTests.ts ui/package.json
git commit -m "Vite dev proxy to axum + vitest config (#15)"
```

---

## Task 5: Typed `/api` data layer

**Files:** `ui/src/api/types.ts`, `ui/src/api/client.ts`, `ui/src/api/client.test.ts`

- [ ] **Step 1: Write the failing client tests**

`ui/src/api/client.test.ts`:

```ts
import { describe, it, expect, vi } from "vitest";
import { getJson, ApiError } from "./client";

describe("api client", () => {
  it("parses JSON on success", async () => {
    vi.stubGlobal("fetch", vi.fn(async () =>
      new Response(JSON.stringify({ templates: [] }), { status: 200, headers: { "content-type": "application/json" } })));
    expect(await getJson("/templates")).toEqual({ templates: [] });
  });

  it("throws ApiError with the error contract on failure", async () => {
    vi.stubGlobal("fetch", vi.fn(async () =>
      new Response(JSON.stringify({ error: { code: "NotFound", message: "nope" } }),
        { status: 404, headers: { "content-type": "application/json" } })));
    await expect(getJson("/templates/x")).rejects.toMatchObject({ code: "NotFound", status: 404 });
  });
});
```

Run: `cd ui && npx vitest run src/api/client.test.ts` → FAIL (no `client.ts`).

- [ ] **Step 2: Implement the client**

`ui/src/api/types.ts`:

```ts
export interface ApiErrorBody { error: { code: string; message: string; details?: unknown } }
export interface TemplateSummary { id: string; name: string; description: string; unit: string; dpi: number; format: { type: string } }
export interface BatchSummary { total: number; succeeded: number; failed: { index: number; error: string }[]; jobs: number }
export interface BatchFailure { index: number; code: string; message: string }
```

`ui/src/api/client.ts`:

```ts
import type { ApiErrorBody } from "./types";

const BASE = "/api";

export class ApiError extends Error {
  code: string;
  status: number;
  details?: unknown;
  constructor(status: number, code: string, message: string, details?: unknown) {
    super(message);
    this.status = status; this.code = code; this.details = details;
  }
}

async function toError(res: Response): Promise<ApiError> {
  const ct = res.headers.get("content-type") ?? "";
  if (ct.includes("application/json")) {
    const body = (await res.json()) as ApiErrorBody;
    return new ApiError(res.status, body.error?.code ?? "Unknown", body.error?.message ?? res.statusText, body.error?.details);
  }
  return new ApiError(res.status, "Unknown", await res.text().catch(() => res.statusText));
}

export async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) throw await toError(res);
  return (await res.json()) as T;
}

export async function sendJson<T>(method: string, path: string, body: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method, headers: { "content-type": "application/json" }, body: JSON.stringify(body),
  });
  if (!res.ok) throw await toError(res);
  return (await res.json()) as T;
}

function filenameFrom(res: Response): string | undefined {
  // Matches the current server's `Content-Disposition: attachment; filename="x"`; not RFC5987 `filename*=`.
  const m = (res.headers.get("content-disposition") ?? "").match(/filename="?([^"]+)"?/);
  return m?.[1];
}

// /api/render/label: 2xx is ALWAYS a binary image/pdf; failure is the JSON error contract.
export async function fetchBlob(path: string, init?: RequestInit): Promise<{ blob: Blob; filename?: string }> {
  const res = await fetch(`${BASE}${path}`, init);
  if (!res.ok) throw await toError(res);
  return { blob: await res.blob(), filename: filenameFrom(res) };
}

// /api/batch: a 2xx is EITHER a binary download (zip/pdf) OR a JSON print summary, depending on `mode`.
// Discriminate on content-type after confirming res.ok; errors are still the JSON contract.
import type { BatchSummary } from "./types";
export type BatchResult =
  | { kind: "download"; blob: Blob; filename?: string }
  | { kind: "summary"; summary: BatchSummary };

export async function submitBatch(body: unknown): Promise<BatchResult> {
  const res = await fetch(`${BASE}/batch`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body),
  });
  if (!res.ok) throw await toError(res);
  const ct = res.headers.get("content-type") ?? "";
  if (ct.includes("application/json")) {
    return { kind: "summary", summary: (await res.json()) as BatchSummary };
  }
  return { kind: "download", blob: await res.blob(), filename: filenameFrom(res) };
}

// Trigger a browser download. Revoke the object URL on a delay, immediate revoke after click()
// can abort the download in Chromium (crbug 41380177); MDN: revoke when finished using the URL.
export function saveBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url; a.download = filename; document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 30_000);
}
```

- [ ] **Step 3: Run to verify it passes**

Run: `cd ui && npx vitest run src/api/client.test.ts` → PASS.

- [ ] **Step 4: TanStack Query provider**

In `ui/src/main.tsx`, wrap the app in a `QueryClientProvider`:

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
const queryClient = new QueryClient();
// ... render <QueryClientProvider client={queryClient}><App /></QueryClientProvider>
```

Add `ui/src/api/queries.ts` with one starter hook used by the shell smoke (templates):

```ts
import { useQuery } from "@tanstack/react-query";
import { getJson } from "./client";
import type { TemplateSummary } from "./types";

export function useTemplates() {
  return useQuery({
    queryKey: ["templates"],
    queryFn: () => getJson<{ templates: TemplateSummary[] }>("/templates"),
  });
}
```

- [ ] **Step 5: Commit**

```bash
git add ui/src/api ui/src/main.tsx
git commit -m "Typed /api client (JSON + binary blob) + TanStack Query (#15)"
```

---

## Task 6: App shell — nav, theme toggle, toasts, routing

**Files:** `ui/src/app/Shell.tsx`, `ui/src/app/App.tsx`, `ui/src/app/ThemeToggle.tsx`, `ui/src/app/toast.tsx`, `ui/src/pages/*`, `ui/src/main.tsx`, `ui/src/app/Shell.test.tsx`

- [ ] **Step 1: Failing shell test**

`ui/src/app/Shell.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { ToastProvider } from "./toast";
import { Shell } from "./Shell";

describe("Shell", () => {
  it("renders the nav sections", () => {
    // Shell renders <ToastRegion/> (needs ToastProvider) and NavLinks (need a Router).
    render(
      <ToastProvider>
        <MemoryRouter><Shell /></MemoryRouter>
      </ToastProvider>,
    );
    for (const label of ["Templates", "Print", "Import", "Settings"]) {
      expect(screen.getByRole("link", { name: label })).toBeInTheDocument();
    }
  });
});
```

Run: `cd ui && npx vitest run src/app/Shell.test.tsx` → FAIL (no `Shell`).

- [ ] **Step 2: Toast system**

`ui/src/app/toast.tsx`: a context + `useToast()` returning `push({ kind, message })`, a provider holding a list, and a `<ToastRegion>` with `role="status"` / `aria-live="polite"` rendering toasts; dedupe identical `kind+message` within 4s. Keep it small and dependency-free (Tailwind classes using the theme vars via inline `style={{ background: "var(--ink)", color: "var(--paper)" }}`).

- [ ] **Step 3: Theme toggle**

`ui/src/app/ThemeToggle.tsx`: a button toggling `document.documentElement.classList.toggle("dark")` and persisting to `localStorage("theme")`. The initial theme is already applied by the pre-paint script in `index.html` (Task 2); the component reads the current `classList.contains("dark")` inside `useEffect`/the click handler, NOT at module scope (keeps it import-safe under jsdom/SSR). Accessible label ("Toggle theme"); visible focus ring (`focus-visible:ring`).

- [ ] **Step 4: Shell**

`ui/src/app/Shell.tsx`: a left sidebar (`<nav>`) with the brand mark and `NavLink`s to `/` (Templates), `/print` (Print), `/import` (Import), `/settings` (Settings), the `ThemeToggle` at the bottom, a main `<Outlet />`, and the `<ToastRegion>`. Active link styling via `NavLink`'s `isActive`. Responsive: below `md`, the sidebar collapses to a drawer toggled by a header button, with a focus trap and `Esc` to close. Use the theme CSS vars for colors (e.g. `style={{ background: "var(--surface)" }}` or Tailwind arbitrary values `bg-[var(--surface)]`). Each `NavLink` must render as a link with its text label (so the test's `getByRole("link", { name })` passes).

- [ ] **Step 5: Placeholder pages + router**

Create `ui/src/pages/{Templates,Print,Import,Settings}.tsx`, each a stub: a heading and, for `Templates.tsx`, a smoke use of `useTemplates()` rendering the count or an error toast (proves the data layer end to end). `ui/src/app/App.tsx` sets up `BrowserRouter` with the `Shell` as the layout route and the four pages (`/`, `/print`, `/import`, `/settings`) plus a catch-all redirect to `/`. Update `ui/src/main.tsx` to render `<App />` inside the providers (Query + Toast).

- [ ] **Step 6: Run tests**

Run: `cd ui && npx vitest run` → all pass (client + shell). `npm run build` succeeds.

- [ ] **Step 7: Commit**

```bash
git add ui/src
git commit -m "App shell: sidebar nav, theme toggle, toasts, routing (#15)"
```

---

## Task 7: End-to-end smoke + docs + gate + merge

**Files:** `docs/SPEC.md`, `README.md`

- [ ] **Step 1: Manual smoke**

```bash
npm --prefix ui run build
cargo run &           # serves :8080, now also serves ui/dist
sleep 3
curl -s -o /dev/null -w "%{http_code}\n" localhost:8080/            # 200 (index.html)
curl -s -o /dev/null -w "%{http_code}\n" localhost:8080/print       # 200 (SPA fallback)
curl -s -o /dev/null -w "%{http_code}\n" localhost:8080/api/health  # 200
curl -s localhost:8080/api/nope | head -c 60                        # JSON NotFound
```

Open `http://localhost:8080/` in a browser: the shell renders, nav works (client-side), the Templates page lists the starter templates (proving the SPA → `/api/templates` path), theme toggle flips light/dark. Stop the server.

Also verify the dev workflow once: `npm --prefix ui run dev` (Vite on :5173) with `cargo run` on :8080 → the Vite app loads and `/api` calls proxy through.

- [ ] **Step 2: Docs**

`docs/SPEC.md`: add a sentence that the root (`/`) serves the React SPA from `ui/dist` (built by Vite); non-`/api` routes fall back to `index.html`; `/assets/*` are the hashed build assets (a missing asset is a 404, not the SPA). `README.md`: add a short "Web UI (dev)" section, `npm --prefix ui install`, `npm --prefix ui run dev` (proxying to `cargo run`), and `npm --prefix ui run build` for the served bundle. (Docker multi-stage build is M6.)

**ADR:** no new ADR is needed. ADR-0008 (Web UI delivery) already records the React-SPA-served-by-axum + `/api` decision this plan implements; note that in the commit message rather than adding/superseding an ADR. (Per CLAUDE.md, behavior changes update SPEC + an ADR; here the ADR already exists, so only SPEC changes.)

- [ ] **Step 3: Gate**

```bash
cargo fmt && cargo clippy --all-targets --all-features 2>&1 | tail -20 && cargo test 2>&1 | tail -10
cd ui && npx vitest run && npm run build && cd ..
```
Backend: clean fmt/clippy, all tests pass. Frontend: vitest passes, build succeeds.

- [ ] **Step 4: Adversarial review + merge**

Per CLAUDE.md, run the reviewer → fix loop on `git diff main...m5-frontend-foundation` (focus: the SPA fallback never shadows `/api` or assets; the binary client's content-type branching; theme toggle SSR-safety / first-paint flash; toast dedupe; no secrets/keys committed; `ui/node_modules` + `ui/dist` ignored). Then:

```bash
git add docs/SPEC.md README.md
git commit -m "Document the web UI serving + dev workflow (#15)"
git checkout main && git merge m5-frontend-foundation && git push
```
Reference `#15`; do not close it (the screens remain).

---

## Self-review notes
- **Spec coverage:** Vite scaffold + Tailwind (T1), Ink & Tape tokens incl. dark + reduced-motion (T2), axum SPA serving with `/api` JSON-404 preserved (T3), dev proxy (T4), typed client with JSON + binary/blob branching + `saveBlob` revoke (T5), shell nav + theme toggle + toasts + routing + a11y focus/drawer/aria-live (T6), smoke + docs + gate (T7). The five real screens, the reusable grid, Docker, and e2e are explicitly the next plan / M6.
- **Type consistency:** `getJson`/`sendJson`/`fetchBlob`/`submitBatch`/`saveBlob`/`ApiError` defined in T5 and used by `queries.ts` (T5) and the shell smoke (T6); `AppState.ui_dir()` + `with_ui_dir` used by the asset `ServeDir` and the `fallback` index.html read (T3).
- **Revised after a codex review (2026-06-16):** UI dir threaded through `AppState` (no global-env race); the `/health` root test replaced with a deterministic isolated-`ui_dir` test; missing-`/assets` returns 404 not SPA html (with a test); `vitest/config` `defineConfig` to keep `npm run build` typechecking; `submitBatch` discriminates 2xx JSON summary vs binary download (`/batch` print vs download); `saveBlob` delays object-URL revoke; pre-paint theme script in `index.html` (no FOUC); Shell test wraps `ToastProvider`; ADR-0008 already covers the decision (SPEC-only).
- **Verify at impl:** Tailwind v3 init (pinned); React 18 vs 19 from the scaffold (either is fine); axum **0.8** / tower-http 0.7 APIs.
