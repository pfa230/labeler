# Contributing

Labeler is a small self-hosted label-rendering service: a Rust/axum backend and a React + TypeScript UI.

## Checks

Run these before submitting a change:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
npm --prefix ui run lint && npm --prefix ui run test && npm --prefix ui run build
```

For UI work, use the Vite dev server (`npm --prefix ui run dev`, port 5173, proxies `/api` to `:8080`). `cargo run` serves the prebuilt `ui/dist` and does not rebuild it, so run `npm --prefix ui run build` after UI changes; the server warns at startup when `ui/dist` is older than `ui/src`.

## Proposing changes

Open an issue first, then a pull request that references it. Behavior is specified under [`openspec/specs/`](openspec/specs/); a change to behavior updates the spec too. The project vision is in [`docs/VISION.md`](docs/VISION.md).
