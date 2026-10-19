## 1. Data-router context

- [ ] 1.1 Convert the actual App route tree to createBrowserRouter/RouterProvider with one route definition and unchanged guards, destinations, redirects and fallback. Adapt editor-consuming unit-test wrappers to data-router context and use the shared App definitions for route tests; verify `/templates` redirects to `/`, unknown paths reach the existing fallback and auth/login/setup behavior remains correct. Missing/moved routes or copied test-only route trees must not count as verification.

## 2. Unsaved YAML guard

- [ ] 2.1 Add the shared YAML-specific blocker/beforeunload component and connect RawYamlSection's existing draft/baseline plus NewTemplate's saved baseline. Update the new-template baseline to the submitted source before success navigation. Extend existing editor tests for dirty versus unchanged/reverted/saved text, failed save, Keep editing retaining text, and Discard reaching the attempted destination; an unconditional guard, premature baseline reset or false clean state must fail. Preserve #144's clean initial copy editor.

## 3. Browser acceptance and gates

- [ ] 3.1 Run the built App in a real browser against deterministic API fixtures using temporary tooling in `.agent-runs/`. Exercise sidebar links and browser Back with both dirty editors, cancel/proceed, saved/unchanged no-prompt paths, successful create navigation, and login/setup, `/templates`, unknown-path and API-401 redirects. After real editor interaction, observe and dismiss/accept native beforeunload on reload; retain scripts/results/trace proving the actual route tree and native dialog were exercised, not only a synthetic DOM event.
- [ ] 3.2 Run UI lint/tests/build and `npx --no-install openspec validate --all --strict --no-interactive`; retain results in `.agent-runs/`.
