## ADDED Requirements

### Requirement: Job history

`GET /api/jobs` SHALL return `200` with every recorded job, for all actors, ordered by descending recorded timestamp and then descending job id. Each job SHALL have the fields below; an empty history SHALL return `[]`.

| Field | Value |
|---|---|
| `id` | recorded integer job id |
| `timestamp` | recorded time as a UTC RFC 3339 string |
| `template_id` | recorded template id |
| `printer_id` | recorded printer id or `null` |
| `status` | recorded status, `ok` or `failed` |
| `error` | recorded error text or `null` |
| `actor_id` | recorded actor id |
| `actor_username` | current username for the recorded actor, or `null` when no user exists for that id |

#### Scenario: An operator sees another actor's job

- **WHEN** jobs from two users include a successful send and a failed send recorded in the same second, and either user reads the history
- **THEN** both jobs appear, the later job id first, with their recorded facts, usernames and complete failure text

#### Scenario: Local jobs remain visible

- **WHEN** a recorded job has actor `local`, which has no user record
- **THEN** the history includes it with `actor_id: local` and `actor_username: null`
