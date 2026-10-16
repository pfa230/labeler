## 1. Correct CSV writes and grid reads

- [ ] 1.1 Use the existing `setOwnKey` for CSV cell assignment and extend `csv.test.ts` with `__proto__`, `constructor` and an ordinary header; verify own-property presence, exact values and JSON serialization, and show that the original direct assignment loses the `__proto__` field.
- [ ] 1.2 Use the existing `getOwnKey` in `LabelGrid`'s data cell and column getter, and extend `LabelGrid.test.tsx` to cover supplied special-name values, a blank missing `constructor` cell and a special-name edit producing an own value; verify the rendered values and changed row, with the missing-cell assertion failing against the original direct read.

## 2. Verify the complete Import path

- [ ] 2.1 Extend the existing Import fixture with matching special-name and ordinary parameters; load CSV, edit a special-name cell and exercise Print and Download using one shared case; verify outgoing JSON preserves both supplied special-name values and the ordinary field, and demonstrate that the original ingestion path fails the regression.

## 3. Run the required gates

- [ ] 3.1 Run `npm --prefix ui run lint`, `npm --prefix ui run test` and `npm --prefix ui run build`; all must pass, including existing ordinary-name, blank/default and batch submission coverage.
- [ ] 3.2 Run `npx --no-install openspec validate --all --strict --no-interactive`; verify the no-delta change passes with `skip_specs: true` and no spec files.
