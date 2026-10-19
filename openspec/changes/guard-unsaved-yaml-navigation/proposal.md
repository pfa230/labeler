## Why

The existing YAML Cancel confirmation does not protect drafts from sidebar navigation, Back or a reload. React Router's data-router blocker is needed to protect unsaved YAML while keeping the application's route and authentication behavior.

Source issue: https://github.com/pfa230/labeler/issues/143

Fixes #143

## What Changes

- Convert the existing route tree to createBrowserRouter/RouterProvider without changing destinations, auth guards or redirects.
- Guard unsaved YAML in Template detail and New template during in-app navigation, with Discard and Keep editing choices.
- Request the browser's native beforeunload confirmation for dirty YAML; unchanged and successfully saved content does not prompt.
- Verify the actual browser route tree, Back behavior, auth redirects and native unload dialog.

## Capabilities

### New Capabilities

### Modified Capabilities

- `ui`: unsaved-YAML navigation confirmation.

## Impact

`ui/src/app/App.tsx`, existing YAML editors, a shared YAML-specific navigation guard and their test wrappers. Keep React Query data access and current template writes. #206 owns view/draft restoration policies; this change adds no persistent state store or new dependency.
