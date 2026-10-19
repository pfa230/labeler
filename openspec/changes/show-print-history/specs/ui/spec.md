## MODIFIED Requirements

### Requirement: Primary navigation

Primary navigation SHALL list Labels (`/`), Import (`/import`), Print history (`/print-history`) and Settings (`/settings`), in that order.

#### Scenario: Navigation sections

- **WHEN** the operator opens the application shell
- **THEN** primary navigation contains exactly Labels, Import, Print history and Settings, in that order

## ADDED Requirements

### Requirement: Print history screen

The Print history page SHALL show the job history (`printing`), with each job's time, template id, printer id, actor and textual send outcome. It SHALL show the actor's username when available, otherwise the recorded actor id, and label an empty actor id as unknown. A failed job's complete error text SHALL be inspectable on the page. `ok` SHALL be described as sent to the printer.

The page SHALL show loading while its first read is pending, an empty-history message for a successful empty response, and the service's message with a Retry action when the read fails.

#### Scenario: An operator traces a failure

- **WHEN** the operator opens Print history containing successful and failed jobs from different actors
- **THEN** each job shows its time, template, printer, actor and send outcome, newest first
- **AND** the failed job's full error can be read without leaving the page

#### Scenario: Empty and failed reads differ

- **WHEN** the initial history read is pending, then succeeds empty, or instead fails
- **THEN** the page respectively shows loading, an empty-history message, or the service's error and Retry
- **AND** Retry performs another history read
