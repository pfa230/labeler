# 8. Web UI delivery

**Status:** Accepted

## Context

The service is a Rust/axum JSON REST API with no web UI yet. Milestone M5 adds the basic operational UI
(template browse + preview, render/print form, CSV import, settings/printers). A "decent, responsive,
themeable UI that avoids a generic, templated look" is an explicit product goal, and a rich GUI template
editor is on the roadmap (Later); the editor research pointed at pdfme/Konva, which are React-centric.
The product also values being a lightweight, single-container, self-hosted deploy.

An initial idea of server-rendering HTML from Rust (templates/macros) was rejected: it is operationally
lean but couples the view layer to the backend language and becomes unmaintainable as the UI grows,
especially with an interactive editor coming. Maintainability and separation of concerns outweigh the
single-binary appeal.

## Decision

- **Separate frontend SPA.** A React + TypeScript single-page app in a `ui/` directory, built with Vite
  to static assets. The backend stays a JSON REST API; the frontend is its own idiomatic codebase
  consuming it. (React over Svelte/Solid so the eventual editor — pdfme Designer / react-konva — shares
  one frontend stack end-to-end.)

- **Serving and routing (one origin).** axum serves the built assets via `ServeDir` and falls back to
  `index.html` for client-side routes. To avoid SPA routes (e.g. `/templates/:id`) colliding with API
  routes, **the REST API is namespaced under `/api`** (`/api/templates`, `/api/render/label`,
  `/api/print`, …), with `/api/openapi.json` and `/api/docs` too; the root is the SPA. This is a
  breaking change to the current root-mounted API, done now while pre-release with no external
  consumers.

- **Docker build.** Multi-stage: a Node stage runs `vite build` → `ui/dist`; the Rust stage builds the
  binary; the final image contains the binary plus `dist/`, served by axum. One image, one process, one
  port. (`rust-embed` to bake `dist` into the binary is an option for a literal single file, at the cost
  of coupling the cargo build to a prebuilt `dist`.)

- **Dev workflow.** `vite dev` with a proxy (`/api` → axum) and hot-reload, so UI iteration does not
  recompile Rust. Production serves the built bundle.

- **Styling.** Tailwind CSS plus headless components (Radix / shadcn-style) for a custom, non-generic,
  responsive look. The exact component library is an M5 implementation detail, not fixed here.

## Consequences

- The M5 shell issue (#15) scaffolds the SPA, the axum static-serving + SPA fallback, and the `/api`
  namespacing; #17/#20/#23/#24 build screens against the REST API. The editor (Later) shares this React
  stack.
- The API surface moves from root to `/api`: handlers/routes, tests, SPEC endpoint paths, the sample
  `scripts/*.sh`, and the inbound webhook path all change accordingly when #15 lands.
- Adds a Node/Vite build-time toolchain (not a runtime dependency) and a multi-stage Docker build.
- **Out of scope:** server-side rendering / meta-frameworks (Next, Remix) and a separately-deployed
  frontend container; both conflict with the single-container, API-only-backend model.
