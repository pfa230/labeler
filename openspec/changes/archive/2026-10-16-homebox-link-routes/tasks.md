## 1. Links

- [x] 1.1 In `src/connector/homebox.rs`, build each link from the row's resource: `/item/<encoded id>` for `entities`, `/location/<encoded id>` for `locations`, for browse rows (`url`, `item_url`, `location_url`) and materialized `item_url`/`location_url`; verify the connector's unit tests assert `/item/` for an entities row and `/location/` for a locations row, in both browse and materialize, and fail against the `/entity/` code
- [x] 1.2 Update the two entities browse tests in `src/lib.rs` that expect `/entity/e1` to expect `/item/e1`; verify `cargo test` passes
