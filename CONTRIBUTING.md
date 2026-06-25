# Contributing

Thanks for your interest. Labeler is a small self-hosted label-rendering service: a Rust/axum backend
and a React + TypeScript UI.

## Building and testing

Run these before submitting a change.

Backend:
```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo test
```

Frontend (from `ui/`):
```bash
npm run lint
npx vitest run
npm run build
```

For active UI work, use the Vite dev server (`npm --prefix ui run dev`, port 5173, proxies `/api` to the
backend on `:8080`); it never touches `ui/dist`. `cargo run` instead serves the prebuilt SPA from
`ui/dist` and does not rebuild it, so run `npm --prefix ui run build` after UI changes or it serves a
stale bundle. The server warns at startup when `ui/dist` is missing or older than `ui/src`.

## Proposing changes

Open an issue to discuss a bug or feature, then submit a pull request that references it. The API and
template schema are specified in [`docs/SPEC.md`](docs/SPEC.md); design decisions are recorded as
[Architecture Decision Records](docs/adr/); the project vision is in [`docs/VISION.md`](docs/VISION.md).
