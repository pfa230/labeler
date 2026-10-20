# Tasks

## 1. Couple registration

- [x] 1.1 Add `utoipa-axum` 0.3 and replace the independent runtime/documentation lists with one documented-handler builder, preserving metadata and the existing middleware/nesting and undocumented surfaces. Serve its collected document through JSON and Swagger and update schema tests to use it. Verify with existing OpenAPI/HTTP tests and a review of before/after documented method/path pairs; add a focused health/document integration test that fails if either side stops using the shared registration. Update the nearby malformed-body-test comment to describe the collected operation source.

## 2. Integration gates

- [x] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test`; require success before committing the refactor.
