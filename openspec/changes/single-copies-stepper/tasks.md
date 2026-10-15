# Tasks

## 1. Copies box

- [ ] 1.1 In `ui/src/pages/print/PrintForm.tsx`, change the Copies input to `type="text"` with `inputMode="numeric"` and remove its `min` and `max`; verify the existing clamp test in `PrintForm.test.tsx` ("clamps the copies stepper to [1, 100]") still passes.
- [ ] 1.2 Add a test to `PrintForm.test.tsx` asserting the box labelled `copies` has role `textbox`, no `spinbutton` is named `copies`, and the `decrease copies` and `increase copies` buttons are present; verify it fails against `type="number"` and passes after 1.1.
- [ ] 1.3 Run `npm --prefix ui run lint && npm --prefix ui run test && npm --prefix ui run build` and `npx --no-install openspec validate --all --strict --no-interactive`; verify all pass.
