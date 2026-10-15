# Proposal

Fixes #444

## Why

The print form's Copies control shows two steppers: its own − and + buttons, and the spin buttons desktop Chrome and Safari draw inside its number box. Both do the same thing, so one is noise.

## What Changes

- The print form's Copies box stops showing browser spin buttons. The − and + buttons stay, since phones draw no spinner and rely on them.
- Connect and Import are unchanged: their Copies boxes have no custom buttons, so the browser spinner is their only stepper.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ui`: adds a requirement that the print form's Copies control offers exactly one stepper.

## Impact

- `ui/src/pages/print/PrintForm.tsx`: the Copies input.
- `ui/src/pages/print/PrintForm.test.tsx`: a test for the new requirement.
