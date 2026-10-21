# Spec Delta

## Purpose

Defines which operator actions produce retained activity entries, the attribution and result each entry exposes, and how clients read that history in bounded pages.

## ADDED Requirements

### Requirement: Selected template activity

Each completed invocation of template create, replace, delete or explicit registry reload SHALL produce one activity entry, including an invocation returning an error. The actions SHALL be `template.create`, `template.replace`, `template.delete` and `template.reload`; their targets SHALL be the requested template id or, for reload, `registry`.

#### Scenario: A template lifecycle has distinct actions

- **WHEN** an operator creates, replaces and deletes template `box`, then explicitly reloads the registry
- **THEN** history contains one entry for each action, the first three targeting `box` and the reload targeting `registry`
- **AND** the internal reloads of template writes produce no additional entries

### Requirement: Activity attribution and result

An entry SHALL expose `id`, `time` in UTC, `actor_id`, `actor_name`, `action`, `target` and `outcome` (`success` or `failure`). Actor identity SHALL be the admitted user's id and username, including a token's owner, or `local` for both fields with authentication disabled. The result SHALL describe the operation's response; a failure does not assert that no change occurred.

#### Scenario: A token acts as its owner

- **WHEN** a token belonging to user `u1` with username `pat` creates a template successfully
- **THEN** its entry identifies actor `u1` and `pat` and outcome `success`

#### Scenario: A rejected template has a failed result

- **WHEN** a local operator replaces a template with invalid YAML
- **THEN** its entry identifies actor `local` and `local` and outcome `failure`

### Requirement: Activity failure detail

A failed entry SHALL include the operation's stable `error_code` and, when present, `error_reason` (`errors`); successful entries SHALL omit both. Activity SHALL store no request body, header, credential, error message or arbitrary error details.

#### Scenario: Invalid YAML is classified without its contents

- **WHEN** invalid submitted YAML contains a distinctive secret value and produces `TemplateInvalid` with reason `template_validation_failed`
- **THEN** the failed entry carries that code and reason, and no stored or returned activity field contains the YAML or its parser message

### Requirement: Recording failure is visible

If recording an entry fails, the selected operation SHALL return `500 Internal` (`errors`) with a message explaining that activity could not be recorded and the requested change may already have happened.

#### Scenario: The change precedes a recording failure

- **WHEN** a template is created but recording its activity fails
- **THEN** the request returns that error even though the template exists

### Requirement: Retained activity pages

All entries SHALL be retained across restarts. `GET /api/activity` SHALL expose shared history, ordered by descending entry id, with at most 100 entries in `entries` and a nullable `next_before_id`. Passing that cursor as `before_id` SHALL return strictly older entries; it SHALL be null when none remain. A supplied cursor SHALL be a positive signed 64-bit integer; other values SHALL return `400 InvalidRequest` with reason `activity_cursor_invalid` (`errors`).

#### Scenario: Older history survives a restart

- **WHEN** 205 entries have been recorded and the service restarts
- **THEN** the newest page contains the newest 100 entries, following its cursor returns the next 100, and following that cursor returns the oldest 5 with no next cursor

#### Scenario: Invalid cursor

- **WHEN** the cursor is `abc`, zero or outside the positive signed 64-bit range
- **THEN** the response is the specified `400` error in the error envelope
