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

## Proposing changes

Open an issue to discuss a bug or feature, then submit a pull request that references it. The API and
template schema are specified in [`docs/SPEC.md`](docs/SPEC.md); design decisions are recorded as
[Architecture Decision Records](docs/adr/); the capability tiers are in
[`docs/CAPABILITIES.md`](docs/CAPABILITIES.md).
