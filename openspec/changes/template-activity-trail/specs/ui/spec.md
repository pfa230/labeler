# Spec Delta

## MODIFIED Requirements

### Requirement: Primary navigation

Primary navigation SHALL list Labels (`/`), Import (`/import`), Activity (`/activity`) and Settings (`/settings`), in that order.

#### Scenario: Navigation sections

- **WHEN** the operator opens the application shell
- **THEN** primary navigation contains exactly Labels, Import, Activity and Settings, in that order

## ADDED Requirements

### Requirement: Activity page

The Activity page SHALL display the retained history (`activity`), showing each entry's time, actor name, action, target, success or failure and failure classification when present. It SHALL open on the newest page and offer Older when a next cursor exists and Newest when viewing older entries. Loading, empty history and read failure SHALL be visible states.

#### Scenario: A failed action can be found

- **WHEN** an operator opens Activity after another user submits invalid template YAML
- **THEN** the newest page identifies that user's failed template action and target, its time, `TemplateInvalid` and `template_validation_failed`

#### Scenario: Browsing older entries

- **WHEN** the operator chooses Older, then Newest
- **THEN** the page first displays entries from the next cursor, then returns to the newest page

#### Scenario: No entries or a failed read

- **WHEN** the initial read is pending, returns no entries, or fails
- **THEN** the page respectively shows loading, an empty-history message, or the read error
