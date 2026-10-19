## 1. Refusal diagnostics

- [ ] 1.1 Correct BrokenTemplate to the existing path/error API shape and add the native disclosure and all-refused empty state to Labels. Extend Templates.test.tsx for mixed printable/refused entries, a count visible while collapsed, expanded full errors, filter-independent diagnostics and an all-refused directory; retain the genuinely empty listing test. Missing errors, filtered-away diagnostics or treating refused entries as printable must fail these assertions.

## 2. Reload and recovery

- [ ] 2.1 Add the existing registry-reload API mutation, header control, pending state and failure feedback, then refresh the templates query after POST success. Extend the existing fetch stub with an explicit reload route and changed registry results: assert POST completes before the subsequent GET and a repaired template replaces its refusal; cover a successful reload that leaves refusals and a failed POST that shows its error while retaining previous results. A list-only refresh, a premature GET, an assumed all-valid success or clearing results on failure must fail these tests.

## 3. Gates

- [ ] 3.1 Run UI lint/tests/build and `npx --no-install openspec validate --all --strict --no-interactive`; retain results in `.agent-runs/`.
