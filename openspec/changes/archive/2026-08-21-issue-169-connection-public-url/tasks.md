## 1. Server: name the field that failed

- [x] 1.1 Add `PublicUrlInvalid => "public_url_invalid"` to the `reasons!` list in `src/reason.rs`,
      next to `BaseUrlInvalid`.
- [x] 1.2 Replace `validate_and_normalize_url`'s `field_name: &str` parameter with a `UrlField` enum
      (`Base`/`Public`) carrying the wire name and the `Reason` (`src/api.rs:1122`), and update both
      call sites in `create_connection` and `update_connection_h`.
- [x] 1.3 Reject embedded userinfo in `validate_and_normalize_url`: a parsed URL with a non-empty
      username or any password fails with that field's reason and a message naming the field.
- [x] 1.4 Update the unit tests in `src/api.rs` (`validate_and_normalize_url_accepts_valid_urls` at
      `:2545`, `validate_and_normalize_url_rejects_invalid_urls` at `:2566`) for the new signature,
      asserting `public_url_invalid` where the field is `Public`.
- [x] 1.5 Add userinfo rejection cases to those tests, covering both `https://user:pass@host` and
      username-only `https://user@host`.
- [x] 1.6 Flip the three integration assertions in `src/lib.rs` that expect `base_url_invalid` for a
      rejected `public_url` (`:417`, `:436`, `:455`) to `public_url_invalid`, and add one covering a
      `public_url` carrying userinfo.

## 2. Server: verify what already exists against the spec

- [x] 2.1 Check each scenario of "Connection record", "Creating a connection", "Updating a
      connection", and "Deleting a connection" against `src/api.rs` and `src/store.rs`, and note any
      scenario with no test behind it.
- [x] 2.2 Add a test only for a scenario found uncovered in 2.1 that falls inside #169's scope. Work
      outside it becomes a GitHub issue, not a task here.
- [x] 2.3 Check "Generated links use the public URL" against `src/connector/homebox.rs`: `browse` and
      `materialize` build links from `external_base_url` (`:128`), and every upstream request goes
      through `base(conn)` (`:123`), which reads `base_url` only.

## 3. UI: the public url field

- [x] 3.1 Add a `publicUrl` state to `ConnectionForm`, initialized from `initial?.public_url ?? ""`,
      and render an optional **public url** input beside **base url** with the same `flex-1` sizing
      and a `https://homebox.example.com` placeholder.
- [x] 3.2 Validate it on submit only when non-blank, with the same `new URL()` parse and
      `http`/`https` protocol check the base url already uses (`ConnectionsSection.tsx:47-49`).
- [x] 3.3 Always send the key: `public_url: publicUrl.trim() === "" ? null : publicUrl.trim()` in the
      submit payload, so blanking the input clears the stored value.
- [x] 3.4 Add a **Public URL** column to the connections table: a header after **Base URL**
      (`ConnectionsSection.tsx:254-259`) and a cell rendering `conn.public_url` or `-` (`:204-210`).

## 4. UI tests

- [x] 4.1 Extend the fetch stub in `ConnectionsSection.test.tsx` to carry `public_url` through
      `POST`, `PUT`, and the list response.
- [x] 4.2 Test setting a public URL: the request body carries it and the table row shows it.
- [x] 4.3 Test clearing it: edit a connection that has one, empty the field, save, and assert the body
      carries `public_url: null` and the row shows `-`.
- [x] 4.4 Test rejecting `homebox.example.com`: the form shows an error and sends no request.
- [x] 4.5 Test creating with the field left empty: the body carries `public_url: null`.

## 5. Decision record

- [x] 5.1 Write `docs/adr/0061-connection-public-url-is-the-link-base.md`: the two-address split, the
      `public_url_invalid` reason, and the userinfo rejection. Mark it as superseding the
      **Connections store** decision of ADR-0018 only.
- [x] 5.2 Add the ADR-0061 row to the index in `docs/adr/README.md`, and note on the ADR-0018 row that
      its connections-store decision is superseded in part by 0061.
- [x] 5.3 Re-check that 0061 is still free against `main` before committing; 0059 and 0060 were taken
      by changes that merged during planning.

## 6. Gates and verification

- [x] 6.1 `npm --prefix ui run lint`, `npm --prefix ui test`, and `npm --prefix ui run build`.
- [x] 6.2 `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test`.
- [x] 6.3 Run `LABELER_CONFIG_DIR=./config-dev cargo run`, set a public URL on a connection through
      the form, and confirm browsed rows link to the public host while the server still fetches from
      `base_url`.
- [x] 6.4 Clear the field on that same connection, save, and confirm the links fall back to
      `base_url`.
