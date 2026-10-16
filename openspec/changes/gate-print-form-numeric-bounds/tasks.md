## 1. Derive bound feedback and action availability

- [ ] 1.1 Add the shared numeric-bound message helper to `labelInputs.ts`; verify its two callers use the same rule and interaction cases distinguish inclusive endpoints, independent zero bounds and blanks without coercion, clamping or default resolution.
- [ ] 1.2 Use the helper in `ParamInput`'s numeric branch to show an adjacent textual bound error, mark the field invalid and associate its message using `useId`; verify the rendered accessible description and invalid marker appear for a violation and disappear on correction or clearing.
- [ ] 1.3 Derive whether any declared numeric field is out of range in `PrintForm`, reading held values with `getOwnKey`, and include that flag in both existing disabled expressions; verify both actions are blocked while any field is invalid and retain the existing busy/stale/printer conditions without changing preview behavior.

## 2. Verify the reported interactions

- [ ] 2.1 Extend the existing `PrintForm.test.tsx` interaction coverage for integer and number controls with lower-only, upper-only and two-sided bounds, including zero, both exact endpoints, correction, an unbounded side, two invalid fields and clearing with/without a default; assert field feedback, no invalid action requests and correct positive request payloads, isolating preview traffic, and demonstrate the tests fail against the old behavior. Repeat typed-input and action-click checks in a current browser after implementation and record availability/request evidence under `.agent-runs/`; retain the existing independent preview-error and blank/default coverage.

## 3. Run the required gates

- [ ] 3.1 Run `npm --prefix ui run lint`, `npm --prefix ui run test` and `npm --prefix ui run build`; all must pass.
- [ ] 3.2 Run `npx --no-install openspec validate --all --strict --no-interactive`; verify the new numeric-bound requirement and the complete modified screen/preview requirements validate without changing the separate list-hint delta.
