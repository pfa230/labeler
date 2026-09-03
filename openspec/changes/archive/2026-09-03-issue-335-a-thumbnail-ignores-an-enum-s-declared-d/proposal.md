## Why

Implements **#335**. `GET /api/templates/{id}/thumbnail` shows an `enum` parameter's *first* declared
value and never the parameter's own `default:`, so a template declaring
`values: [horizontal, vertical], default: vertical` and printing `{orientation}` previews as
`horizontal`. The thumbnail is the last production caller of the retired preview-only option map, and
it is that map, merged into the request data ahead of every declared default, that decides the value.
No other parameter type is treated this way, and no caller can send such a map, so the preview is the
only place a template's declared default is overruled by the service.

## What Changes

- The preview-only option selection is deleted, together with the merge that placed it ahead of a
  declared default. The thumbnail's placeholder rule stays `interpolated && required`, with one
  addition: a `select` entry is filled only where its parameter declares **no** `default:`. An `enum`
  that declares one therefore resolves from the request's `data`, else that default, else absent.
- **BREAKING** A thumbnail of a template printing an `enum` that declares a `default:` shows that
  default rather than the first of its `values`.
- A thumbnail invents a value for an `enum` that an active item prints and that declares **no**
  `default:`: the first of its `values`. Such a template still previews rather than failing on an
  unresolved token, and the invented value is legal for the parameter.
- **BREAKING** An `enum` that declares no `default:` and that only a `when:` key names is absent in a
  thumbnail, so the item it gates does not draw. The option selection used to activate it.
- **BREAKING** A thumbnail whose active item references an undefaulted `enum` through a colour
  (`color`, `background`, `stroke.color`) or dimension (`{ref}` in `size`, `at`, `to`, `width`/`height`)
  now fails where it rendered: `400 InvalidRequest` with `color_param_invalid` or `missing_field`.
  `placeholder_data` only fills `interpolated` names (`src/templates.rs:163`) and
  `TemplateContent::options()` (`src/templates.rs:82-94`) walks every declared `enum` regardless of
  whether a token reads it, so the deleted `default_option_selection` was the only source that supplied
  these `interpolated: false` names. A caller's render of such a template succeeds when it supplies the
  `enum` value.
- **BREAKING** A thumbnail of a template whose `enum` `default:` cannot be resolved fails with
  `422 TemplateInvalid` and `param_default_unresolvable`, naming the parameter. The first-value
  stand-in used to mask it, so a broken template previewed as a healthy one. That is the one place the
  `select` addition bites: because a broken default leaves an entry `required`, a broken default on
  every *other* control is still masked by that control's placeholder and its thumbnail still renders.
  Whether that split is right is **#344**; this change publishes it and states it in both capabilities.
- No caller can supply an option map, so after this change nothing populates the renderer's internal
  option-selection argument. Deleting that plumbing is **#214** and is not part of this change.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `template-inputs`: **The thumbnail renders the default selection from placeholder data** gains the
  `select` fill rule and loses the option selection, and its two printed-enum scenarios are replaced.
  **A screen renders the reported inputs and decides nothing else** is corrected where it describes
  what a thumbnail passes alongside its data; what a *client* fills is unchanged.
- `param-resolution`: **A preview invents values, and says which ones, because no caller supplied
  any** loses the rule that gave every declared `enum` its first value ahead of any declared default.
  **A parameter is required unless the template declares a default** is corrected where it describes
  what populates the option-selection argument.

## Impact

- `src/templates.rs`: `placeholder_data` fills a `select` entry instead of skipping it.
- `src/render/mod.rs`: `default_option_selection` is deleted; the option merge in
  `resolve_parameters_mode` is deleted, and with it the now-unread `option` argument on
  `resolve_parameters` / `resolve_parameters_mode`.
- `src/api.rs`: the `thumbnail` handler stops building an option selection.
- Tests: the thumbnail and placeholder tests in `src/templates.rs` and `src/render/mod.rs`, the
  `default_option_selection` unit test (deleted with its subject), and the render-dumping harness that
  iterated orientations through the option map.
- API surface: no request or response model changes. No UI change.
- Behavior visible in the catalog grid: a template whose enum is only a gate key and declares no
  default previews without that branch. `tests/fixtures/templates/avery5163_asset_tag.yaml` (`outline:
  [yes]`, no default) and `tests/fixtures/templates/container_circle_gated.yaml` (`enabled:
  [yes, no]`, default `no`) are that shape: the outline and the stroked circle drew in thumbnails
  because `default_option_selection` forced `outline: yes` / `enabled: yes` ahead of the declared
  default (or absence), and now they do not. The rule is pinned by synthetic gate tests
  (`src/templates.rs` `thumbnail_enum_only_gate_*`, `src/lib.rs` HTTP twin); `every_template_renders`
  continues to assert the id set and PNG magic only and passes with the circle either way. Nothing
  under `catalog/` declares an `enum`.

## Out of scope, and why

1. **The client-side preview fills every declared `enum` with its first allowed value**
   (`ui/src/lib/preview.ts`), including one that declares a `default:`, so a template preview keeps
   showing `horizontal` where the thumbnail will show `vertical`. That is the same defect on the other
   path and its own behavior change, outside #335's acceptance criteria. Filed as **#343**. (#215,
   which the published `template-inputs` text names, is closed and covered the older shape: filling a
   declared parameter with its own *name*.)
2. **Whether a preview should mask a broken default at all** is **#344**. What this change does settle
   is what the two capabilities *say*: `param-resolution` claimed that a parameter declaring a
   `default:` is never stood in for, which is false for every control but `select`
   (`src/templates.rs`, `thumbnail_tests_for_new_default_rules`, case 5), and that claim and the one
   scenario resting on it are corrected here, because the precise rule is what this change publishes.
   The behavior is untouched and the choice between masking everywhere, failing everywhere and keeping
   the split is #344's.
3. **Deleting the option-map plumbing** left dead by this change is #214.
