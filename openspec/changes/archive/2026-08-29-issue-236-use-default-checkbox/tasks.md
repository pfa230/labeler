## 1. Form state

- [x] 1.1 Add a deferral set to `FormValue` in `ui/src/pages/print/FieldForm.tsx`, keyed by entry `name`
- [x] 1.2 Initialise it in `ui/src/pages/print/PrintForm.tsx` so every entry publishing a `default` starts deferred, alongside the existing `initialDataFromInputs`
- [x] 1.3 Add an entry to the deferral set when it first appears in a later list, retain its state when it leaves, and restore it if it returns
- [x] 1.4 Reinitialise both `data` and the deferral set from the new template's `inputs.default` when `detail.id` changes, so a shared name carries nothing across templates

## 2. Submission

- [x] 2.1 Omit deferred names from `submittedData` in `ui/src/pages/print/PrintForm.tsx:78`, so the preview, both downloads and both prints follow with no per-site change
- [x] 2.2 Send the same map to the input-list request at `PrintForm.tsx:46-50`, which today sends raw `value.data`

## 3. The control

- [x] 3.1 Render the `Use default` checkbox in `FieldForm` for every entry publishing a `default`, checked when the entry appears, naming the published default as text
- [x] 3.2 Give the checkbox an accessible name containing the entry's `name`, and stop the value control and the checkbox sharing one `<label>` element
- [x] 3.3 Disable the value control while the entry is deferred, leaving what the seeding rule put there untouched
- [x] 3.4 On re-checking, discard any value entered while the checkbox was cleared
- [x] 3.5 On re-checking an `image` entry, clear the uncontrolled file input's own selection (`ui/src/components/ParamInput.tsx:48-64`)

## 4. Tests

- [x] 4.1 A deferred entry is absent from the submitted `data` and from the list request; an undeferred one is present
- [x] 4.2 A published default no control can hold (`"80mm"` on `number`, and an `image` entry) still defers, and no key is sent for either
- [x] 4.3 Clearing the checkbox submits what the control holds; re-checking discards it and sends no key
- [x] 4.4 Re-checking an `image` entry clears the file chooser's selection
- [x] 4.5 An entry appearing after a branch switch arrives deferred; one that leaves and returns keeps its cleared state
- [x] 4.6 Switching to a template sharing an entry `name` carries neither value nor deferral across
- [x] 4.7 Two entries sharing a `description` and a `default` have different accessible names, each containing its entry's `name`
- [x] 4.8 An entry publishing no `default` renders no checkbox and behaves as before

## 5. Record

- [x] 5.1 Write `docs/adr/0090-a-declared-default-is-deferred-not-copied.md` and add its row to `docs/adr/README.md`

## 6. Gates

- [x] 6.1 `cargo fmt`
- [x] 6.2 `cargo clippy --all-targets --all-features`
- [x] 6.3 `cargo test`
- [x] 6.4 `npm test` and `npm run lint` in `ui/`
