## 1. Driver and API

- [ ] 1.1 Parse `cut` from `operations-supported` in `PrinterCapabilities::from_attributes` and add it to the probe response; verify with unit tests that enum `0x5f01` as a set member or scalar yields `cut: true` and its absence `cut: false`.
- [ ] 1.2 Add `cut_request(uri)` and `PrinterDriver::cut`, implemented by `CupsDriver` and by `FakeDriver` through a `cut` knob; verify with a unit test that the request carries operation code `0x5f01`, the printer's `printer-uri` and no document data.
- [ ] 1.3 Add `POST /api/printers/{id}/cut` returning `{status: "ok"}` or `{status: "failed", detail}`, registered in `src/openapi.rs`; verify with API tests for a successful cut, a refused cut whose detail names the IPP status, and `404` for an unknown id.

## 2. MCP

- [ ] 2.1 Add the `cut_printer` tool with its schema, annotations and description, validating the printer id before REST; verify with MCP tests that the tool list has twelve tools, `printer: "../other"` errors without an HTTP request, and a `failed` response returns `isError: true` with the JSON unchanged.
- [ ] 2.2 Add `cut_printer` to `docs/MCP.md`'s tool table and id rules; review the diff.

## 3. UI

- [ ] 3.1 Add the cut call and `cut` capability to `ui/src/api/` and a Cut button beside Print in `PrintForm.tsx`; verify with `PrintForm.test.tsx` cases that the button appears only when the probe reports `cut: true`, pressing it posts to the selected printer's cut endpoint, and a failed cut shows an error notice with the detail.

## 4. Gates

- [ ] 4.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `npm --prefix ui run lint && npm --prefix ui run test && npm --prefix ui run build` and `npx --no-install openspec validate --all --strict --no-interactive`; all pass without changing files.
