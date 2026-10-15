## 1. Request and capability handling

- [x] 1.1 Add the strict `chain_and_continue` boolean with false default to `PrintRequest` and the UI print body type; verify omission, false and true are accepted, non-boolean/null values and the field on render endpoints get `json_malformed`, and OpenAPI reflects the accepted request.
- [x] 1.2 Add only trim and chain-and-continue support flags to existing printer capability parsing and expose the continuation boolean in the existing probe response; verify scalar/set advertisements, the required collection member plus template, absent/wrongly typed values, enum-only printers, failed probes and existing capability fields.
- [x] 1.3 Add the bodyless stored-printer probe using the stored connection, existing driver and existing result shaping, and register it in OpenAPI; verify known/unknown/unreachable printers, saved-password authentication, preserved transport settings, no password in responses, zero print submissions, and no stored-record mutation.
- [x] 1.4 Carry the continuation boolean through the existing printer target, probe once per print request even with both render overrides, and reject unsupported true before rendering; verify the stable error constructor/reason, a capability change since the UI probe, refusal outranking bad label data with zero dispatch, normal false/omitted behavior, and unchanged render override precedence.

## 2. IPP dispatch

- [x] 2.1 Add one optional driver finishing choice and serialize either trim enum 11 or one `finishings-col` collection with keyword `finishing-template: ptouch-chain`; verify serialized/decoded job attributes use exactly one representation or neither, preserving integer copies, true attribute fidelity, payload, title, document format and media dimensions.
- [x] 2.2 Capture actual outgoing requests through `/api/print`; verify one checked entry with copies 3 produces one chain job and three distinct checked entries produce three chain jobs including the last, ordinary supported requests use trim on every job, unsupported ordinary requests omit finishing, and copies, true attribute fidelity, physical-label summary counts, media geometry and sheet/PDF behavior remain intact. Inject intermediate/last send failures and verify unchanged summaries/job logs, preserved order, and no extra cut or replay after failed or accepted jobs.

## 3. Print form

- [x] 3.1 Query the selected stored printer through the existing query library and add an initially unchecked Chain and continue checkbox beside Print only on confirmed support; verify supported/unsupported/loading/unreachable cases, ordinary Print availability, true-only submission with one label entry carrying copies, unchecked omission, state reset on printer/support changes, stale responses from a prior printer, and existing error presentation. Use existing controls and concise held-end help; offer no no-cuts, final-cut-only or precut setting.
- [x] 3.2 Capture preview and download requests with continuation checked; verify no continuation field reaches render requests and existing data, format and sheet slots are preserved.

## 4. Gates

- [x] 4.1 Run the cut pass and review the diff, naming each test's observable failure condition; verify `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `npm --prefix ui run lint`, `npm --prefix ui run test`, `npm --prefix ui run build`, and `npx --no-install openspec validate --all --strict --no-interactive` pass. Keep implementation local until spec sync and archival through the supervised loop are complete.

## Owner handoff before archival

Not an implementation or verification task; the autonomous run neither performs nor checks it. After the tasks above and their review are complete and the loop's integration stage has finished, the loop stops before archival and hands off to the owner. The owner prints through the changed labeler path on the PT-2730: a checked three-copy job, checked multi-entry continuation, another checked print after an idle gap, and an unchecked subsequent print. The owner records the PT-2730 revision, submitted attributes, ordered jobs and the observed physical results under `.agent-runs/`: separated labels, the held last end, release by the next print, initial leader only on a fresh run, and ordinary finalization. Archival advances only after those recorded results pass and their evidence reference is committed; if they are pending or fail, the change stays unarchived.

Accepted by the owner on 2026-10-15 after printing through the changed labeler path on the PT-2730; the printer revision, attributes and per-print observations were not recorded.
