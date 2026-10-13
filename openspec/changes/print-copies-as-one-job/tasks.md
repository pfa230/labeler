## 1. Request and rendering

- [ ] 1.1 Add `copies` (`NonZeroU32`, default 1) to `LabelInput` with an integer OpenAPI schema; verify that omission and `copies: 2` are accepted, that `0`, `-1`, `1.5` and `null` get `json_malformed`, and that the generated document declares `copies` optional, non-nullable, integer, minimum 1 and default 1.
- [ ] 1.2 Count labels as a checked sum of `copies` before expanding or rendering anything, and use that count for the cap and `total`; verify two entries of 250 are accepted, two of 300 get `413`, and one entry of `copies: 4294967295` gets `413` without rendering.
- [ ] 1.3 Number labels after expansion and render each `single` entry once: ZIP entries per label, one print unit per entry with its indices and count, and sheets filled from expanded labels; verify the rendering spec's copies scenarios (ZIP names and bytes, A crossing from page 1 to page 2 at `start_slot` 22, a failing entry reported at each of its indices), and that an entry with `copies: 3` yields one print unit carrying indices 0–2 and count 3.

## 2. IPP dispatch

- [ ] 2.1 Add `copies` to `PrintOptions` and emit `ipp-attribute-fidelity: true` in the operation group and integer `copies` in the job group from `print_job_request`; dispatch one job per unit with its count, a sheet job with 1, and fail all of a unit's indices when its job fails.
- [ ] 2.2 Add the loopback IPP endpoint for tests and capture `/print` requests through the real `CupsDriver`; verify each printing scenario: one job with `copies` 3 and fidelity true in the operation group for a three-copy entry, `copies` 1 for one copy, two ordered jobs with `y-dimension` exactly each label's length and each label's own payload and `document-format` for two three-copy entries, one sheet PDF job with `copies` 1, a refused copies job reporting `total` 4, `sent` 1, `jobs` 2 and failed indices 1–3, and a connection closed before the reply recording exactly one job and failed indices 0–2.

## 3. UI

- [ ] 3.1 Send each label once with `copies`: the print form's one entry, and one entry per row from `resolveLabels` for Connect and Import print, download and sheet preview; count labels as rows times Copies for the preview cap and the download message, and include `copies` in `sheetPreviewKey`. Verify the print form sends one entry with `copies: 3`; the sheet preview and Download send 3 entries with `copies: 2` and equal `start_slot`; changing Copies after a completed sheet preview sends a new preview request; Connect and Import each report "Downloaded 6 labels" for 3 rows at Copies 2; and on each page a summary failing indices 3–5 with Copies 3 marks row 2 only.

## 4. Acceptance and gates

- [ ] 4.1 Through labeler's `/print`, print to the PT-2730 one label with three copies and a two-label request with three copies each; record the submitted attributes, the job count and the owner-confirmed physical result (label count, cuts) under `.agent-runs/` before claiming hardware success.
- [ ] 4.2 Run the cut pass, and name for each new test the regression that makes it fail; verify `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `npm --prefix ui run lint`, `npm --prefix ui run test`, `npm --prefix ui run build` and `npx --no-install openspec validate --all --strict --no-interactive` pass.
