## 1. Make list-default hints explicit

- [ ] 1.1 Change the existing FieldForm hint expression to format array defaults as compact JSON and scalar defaults as before; verify only the hint expression changes, with the existing prefix and absent-default/checkbox conditions intact.
- [ ] 1.2 Add a table-driven rendered-hint regression in the existing FieldForm suite for `["A", "B"]`, `["A,B"]`, `[""]`, `[]` and `["B", "A"]`, using literal expected strings; verify all are distinct where required and preserve order, and demonstrate that the original `String` expression fails the cases.
- [ ] 1.3 Update the existing PrintForm list-default hint expectation and retain its untouched-omission and explicitly emptied-`[]` assertions; verify that test and existing scalar/default and ordered-list submission tests pass without changing form state or request construction.

## 2. Run the required gates

- [ ] 2.1 Run `npm --prefix ui run lint`, `npm --prefix ui run test` and `npm --prefix ui run build`; all must pass, including the rendered hints and existing submission coverage.
- [ ] 2.2 Run `npx --no-install openspec validate --all --strict --no-interactive`; verify the complete modified Parameter controls requirement and all retained scenarios validate.
