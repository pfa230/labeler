## Purpose

Defines how the templates directory is turned into the served template registry: which files are read, what happens to a file whose content is unusable, how two files claiming one id resolve, and how the refused files are reported to the operator. The registry is rebuilt from disk by this same contract at startup and on every reload.

## ADDED Requirements

### Requirement: Startup survives every template content fault

The service SHALL start whenever the templates directory is readable, regardless of the content of the files in it. A file that fails to parse, fails validation, or claims an id another file already holds SHALL be excluded from the served set and reported as broken, and SHALL NOT abort startup.

The service SHALL abort startup only when the templates directory itself cannot be read or enumerated, or an individual file cannot be read.

This requirement supersedes the `docs/SPEC.md` §3 sentence "Parsing rejects unknown fields (`deny_unknown_fields`). An invalid template aborts server startup." for its second sentence only; unknown-field rejection is unchanged.

#### Scenario: A malformed file does not stop the service

- **WHEN** the templates directory holds one valid template and one file that is not valid template YAML
- **THEN** the service starts
- **AND** the valid template is served
- **AND** the malformed file is reported as broken

#### Scenario: An unreadable templates directory is fatal

- **WHEN** the templates directory cannot be read
- **THEN** the service exits with a fatal error naming the directory

### Requirement: Templates load in a deterministic order

The registry SHALL load the files of the templates directory whose extension is `yaml` or `yml`, compared case-insensitively, in ascending byte order of filename, independent of the order the filesystem enumerates them. Files with any other extension SHALL be ignored.

Two loads of the same directory contents SHALL therefore produce the same served set, the same id-to-file mapping, and the same broken list, on any machine.

#### Scenario: Load result does not depend on filesystem order

- **WHEN** the same set of template files is loaded on two machines whose directory enumeration order differs
- **THEN** both loads serve the same templates from the same files
- **AND** both report the same broken files

### Requirement: A duplicate template id refuses the colliding file

When two or more files in the templates directory declare the same template `id`, the registry SHALL serve the one whose filename sorts first in byte order (so `Z.yaml` sorts before `a.yaml` on a case-sensitive filesystem) and SHALL refuse each later file for that id.

A refused file SHALL:

- be excluded from the served set, leaving the winning file's template served and unchanged;
- appear in the broken list, identified by its own filename;
- carry an error message naming the duplicated id, and the filename of the file it collides with.

Refusing a colliding file SHALL NOT eject, alter, or invalidate the template already accepted for that id, and SHALL NOT affect any other template in the directory.

A duplicate id SHALL NOT produce an HTTP error response. The `template_duplicate_id` reason of `docs/SPEC.md` §10.1 therefore no longer reaches the wire: it names the cause carried in a `broken` entry's message, and no request can provoke a `TemplateInvalid` response carrying it.

This requirement supersedes the `docs/SPEC.md` §3 top-level field table entry for `id` ("Required, non-empty, unique across the directory") to the extent of what a collision does: the id remains required and non-empty, and uniqueness is now enforced by refusing the colliding file rather than by refusing to run. It also supersedes the §10.1 row for `template_duplicate_id` as to where that reason appears.

#### Scenario: Two files declare one id

- **WHEN** `alpha.yaml` and `zed.yaml` both declare `id: badge` and both are otherwise valid
- **THEN** the service starts
- **AND** `badge` is served from `alpha.yaml`
- **AND** `zed.yaml` is reported as broken with a message naming `badge` and `alpha.yaml`
- **AND** every other template in the directory is served normally

#### Scenario: A colliding file cannot evict the served template

- **WHEN** a directory serving `badge` from `alpha.yaml` gains `zed.yaml`, which also declares `id: badge`, and the registry is reloaded
- **THEN** `badge` is still served from `alpha.yaml`
- **AND** `zed.yaml` is reported as broken

#### Scenario: A collision resolves once the operator fixes it

- **WHEN** the operator changes the id in the refused file, renames it, or deletes it, and reloads the templates
- **THEN** the collision is gone from the broken list
- **AND** the surviving files are served

### Requirement: Refused templates are reported, not silently dropped

Every file the registry refuses SHALL be reported through all three of these channels:

- a warning log line at startup, carrying the file and the reason it was refused;
- the `broken` list of `GET /api/templates`, as `{ filename, error }` entries carrying that same
  reason, omitted when empty;
- the `broken_count` of the `POST /api/templates/reload` response, alongside `count`, counting the
  refused files.

`POST /api/templates/reload` SHALL succeed and swap in the new registry whenever the templates directory is readable, including when some files were refused. It SHALL NOT fail on refused files.

This requirement supersedes the `docs/SPEC.md` §2.0 bullet "`POST /templates/reload` re-scans the dir and returns `{ "count": N }`", and the §2.0 sentence "A reload that fails (an invalid file on disk) returns `422` and keeps the previously-loaded set, so a bad file never takes the service down." A reload now fails only when the directory or one of its files cannot be read; it then returns `500 RenderFailed` with `details.reason` `template_registry_io` and keeps the previously-loaded set.

#### Scenario: Reload reports the collision instead of failing

- **WHEN** a duplicate id is created on disk and `POST /api/templates/reload` is called
- **THEN** the response is `200` with `count` for the served templates and `broken_count` including the refused file
- **AND** `GET /api/templates` lists the refused file under `broken` with its message

#### Scenario: Reload keeps the live set when the directory is unreadable

- **WHEN** the templates directory cannot be read and `POST /api/templates/reload` is called
- **THEN** the request returns `500 RenderFailed` with `details.reason` `template_registry_io`
- **AND** the previously-loaded templates stay served

### Requirement: A `422` from a template write means nothing was written

`POST /api/templates` and `PUT /api/templates/{id}` SHALL return `422 TemplateInvalid` only for a submitted body that fails parsing or validation, which is rejected before anything is written. The directory re-read that follows a successful write SHALL NOT produce a `422`: the only fault it can still raise is an unreadable directory or file, which is a `500`, and the file stays written.

`DELETE /api/templates/{id}` SHALL NOT return `422`. A refused file elsewhere in the directory SHALL NOT block a delete.

This requirement supersedes the `docs/SPEC.md` §2.0 `PUT /templates/{id}` bullet's account of a `422` having two sources with different effects, including "Callers must not read a `422` as 'nothing was saved'", and the §2.0 `DELETE /templates/{id}` bullet's `422` clause. It also supersedes the §2 endpoint table's success column for `POST /templates/reload` (`200 {"count":N}` / `422`), which now reads `200 { "count": N, "broken_count": N }` / `500`.

#### Scenario: An invalid body is rejected before the write

- **WHEN** `PUT /api/templates/{id}` receives a body that fails validation
- **THEN** the response is `422`
- **AND** the stored file is unchanged

#### Scenario: A refused sibling does not block a delete

- **WHEN** `DELETE /api/templates/{id}` is called on a served template while another file in the directory is refused
- **THEN** the response is `204`
- **AND** the refused file is still reported as broken
