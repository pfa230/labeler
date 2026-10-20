# Proposal

## Why

Operators cannot see who changed a template or activated files through a registry reload, nor distinguish successful changes from rejected attempts. A browsable template activity trail supplies that history without expanding print-job history.

Fixes #94

Source issue: https://github.com/pfa230/labeler/issues/94

## What Changes

- Record completed template create, replace, delete and explicit registry-reload operations with time, actor, target and outcome; retain useful error classifications without request bodies or credentials.
- Retain all entries and expose newest-first history through an authenticated API in pages of 100.
- Add an Activity page to primary navigation, including access to older entries.
- Return an explicit error if recording fails, explaining that the requested change may already have happened.

## Capabilities

### New Capabilities

- `activity`: Retained records of selected operator actions and their bounded read contract.

### Modified Capabilities

- `ui`: Primary navigation and the Activity history page.
- `errors`: The invalid activity-cursor reason slug.

## Impact

Touches SQLite migrations and Store methods, the four template mutation handlers, API models and OpenAPI registration, and the UI router, shell, client and new Activity page. Uses existing SQLite, authentication and UI components without new dependencies.

Non-goals: printer, settings, variable and authentication events are unselected candidates; print history belongs to #407. Existing print-job recording and Recent templates stay as they are. No retention cap, pruning job, audit configuration, filters or export.
