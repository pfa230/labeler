## 1. Red: new failing tests for the new contract

- [x] 1.1 Add a `wrap: true` range test where a word is too wide at `max` but fits whole in range: assert it renders whole on one line at the largest such size (delta `An over-wide word shrinks whole instead of breaking`), and show it failing against the chunking implementation
- [x] 1.2 Add `wrap: true` floor tests for an over-wide word at `min`: `ellipsis` renders the shortened `...` form and `fail` returns `text_does_not_fit` (delta `An over-wide word at the floor is shortened when the marker still fits`), and show them failing
- [x] 1.3 Add `wrap: true` fixed-size tests for an over-wide word: `ellipsis` shortens with the marker instead of splitting, `fail` returns `text_does_not_fit`, and a box narrower than `...` fails under `ellipsis` (delta `An over-wide word at a fixed size takes the overflow outcome`), and show them failing
- [x] 1.4 Add a `wrap: true` test where the first line is over-wide and the last fits with nothing dropped: assert the marker sits on the shortened line and the last line is emitted untouched (delta `An over-wide line is shortened where it sits, not only at the end`), and show it failing
- [x] 1.5 Add HTTP tests asserting emitted lines and status for the shrink-to-fit case and the floor `fail` (`text_does_not_fit`) case, and show them failing

## 2. Implementation

- [x] 2.1 Delete both character-chunking loops in `wrap_text` (`src/render/helpers.rs:910-925` and `941-954`) so an over-wide word stays whole on its own line; change nothing in `text_fits`/`largest_fitting_font` or the ellipsis path
- [x] 2.2 Rewrite `layout_text_ellipsizes_every_over_wide_line_not_only_the_last` (`src/render/helpers.rs:1602-1637`) with a value that wraps without chunking (two over-wide words or a hard break), keeping its block-fits assertions; do not delete it
- [x] 2.3 Run the new tests from 1.1-1.5 green and confirm the full suite passes with `wrap: false` tests untouched

## 3. Gates

- [x] 3.1 Run `cargo fmt --check` and repair any formatting as a normal edit
- [x] 3.2 Run `cargo clippy --all-targets --all-features` and fix the root cause of any lint
- [x] 3.3 Run `cargo test` (unit + HTTP integration) green
