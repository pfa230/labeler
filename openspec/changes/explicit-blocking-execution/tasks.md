# Tasks

## 1. Express synchronous execution

- [ ] 1.1 Make Store methods synchronous, add one private mutex-guard accessor, and update callers and helpers that only awaited Store; simplify HTTP test token seeding accordingly. Update the Store API comment to state that SQLite blocks the caller. Verify with the existing Store, authentication, settings and HTTP tests through `cargo test`; forgotten awaits must fail compilation and changed persistence or responses must fail those existing assertions.
- [ ] 1.2 Document inline Typst execution and the small local workload assumption beside the thumbnail, single-label and batch render paths. Verify by reviewing each entry point against its synchronous render call; introduce no scheduling code.

## 2. Integration gates

- [ ] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test`; require success before committing the refactor.
