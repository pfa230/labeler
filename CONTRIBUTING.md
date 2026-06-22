# Contributing

Thanks for your interest. This is a small self-hosted label service (Rust/axum + a React UI).

## Dev gates (run before any change is done)

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

## Templates are visual

A template YAML that parses and renders without error is not proof it looks right. When authoring or

locally) and inspect the image against intent, then fix and re-render until correct.

## Workflow


`Refs #N` / `Fixes #N`. The living spec is [`docs/SPEC.md`](docs/SPEC.md); decisions are
[Architecture Decision Records](docs/adr/); the capability tiers are
[`docs/CAPABILITIES.md`](docs/CAPABILITIES.md).
