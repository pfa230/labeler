# 56. Parameterized templates and dynamic layout constraints

Date: 2026-08-17

## Status

Accepted. Issue [#162](https://github.com/pfa230/labeler/issues/162). Supersedes
[ADR-0005](0005-recursive-containers-with-option-gating.md) (option gating) and refines
[ADR-0010](0010-variable-interpolation-layer.md), [ADR-0012](0012-job-options.md), and
[ADR-0022](0022-import-option-model.md).

## Context

Historically in Labeler, template inputs were split across multiple disconnected mechanisms:
- **Layout and format attributes**: Strictly static literals in YAML (e.g. `format.width.max`,
  `font_weight`, container sizes, padding).
- **`options: { ... }`**: Discrete string enums used exclusively by `container.option` for conditional
  subtree rendering.
- **Content bindings**: Un-typed string tokens resolved via `value: "{field}"` (and legacy `name: field`).

This split created several architectural limitations:
1. **No dynamic layout attributes or formatting constraints**: A template could not parameterize
   its target tape length (`target_width`), font weight (`weight`), or box dimensions per request.
2. **Limited gating model**: Option gating was exclusive to `container` items. Gating a single `text`,
   `qr`, `image`, or `line` required wrapping it in a superfluous container.
3. **No typed input constraints or UI metadata**: Templates had no way to declare typed parameters
   (`string`, `length`, `integer`, `number`, `boolean`, `enum`), default values, ranges (`min`/`max`),
   multiline hints, or descriptions. Consequently, the UI had to infer form controls heuristically.
4. **Eager missing-field validation**: Missing required fields in inactive option branches could trigger
   `422 MissingField` errors even when those branches were never rendered or measured.

## Decision

**1. Unified parameter declaration (`params:`)**
All template inputs—content fields, layout constraints, styling parameters, and conditional switches—are
consolidated under a single top-level `params:` block:

```yaml
params:
  message:
    type: string
    description: "Main label text"
    multiline: false
  notes:
    type: string
    multiline: true
    default: ""
  target_width:
    type: length
    default: 80
    min: 25
    max: 300
    description: "Target length"
  weight:
    type: integer
    default: 400
    enum: [100, 200, 300, 400, 500, 600, 700, 800, 900]
  show_border:
    type: boolean
    default: false
  orientation:
    type: enum
    values: [horizontal, vertical]
    default: horizontal
```

Parameter types supported:
- `string`: Accepts `default`, `multiline` (bool), `description`.
- `length`: Dimension value in template units; accepts `default`, `min`, `max`, `description`.
- `integer`: Whole number; accepts `default`, `min`, `max`, `enum` (allowed integer list), `description`.
- `number`: Floating-point value; accepts `default`, `min`, `max`, `description`.
- `boolean`: Boolean flag; accepts `default` (defaults to `false` when omitted without default), `description`.
- `enum`: Discrete string choice; accepts `values` (non-empty string list), `default` (defaults to first value when omitted), `description`.

**2. Reserved parameter names and validation**
Parameter names must match `^[a-zA-Z0-9_-]+$`. The namespaces `datetime`, `vars`, and any name
containing a dot (`.`) are reserved and rejected at template load time (`422 TemplateInvalid`).

Token substitution precedence in `value` expressions:
1. `{datetime}` / `{datetime.<format>}`
2. `{vars.<key>}`
3. `{param_name}` (resolved from request `data` + parameter `default`s)

**3. Dynamic format and layout attributes (`DynamicValue<T>`)**
Format dimensions (`format.width`, `format.height`) and layout attributes (`font_weight`, `size`, `padding`)
accept either a literal constant or an exact `"{param_name}"` reference.

At template load time, layout bounds validation instantiates parameter defaults to verify that the default
configuration produces valid geometry. At render time, requested parameter values (falling back to defaults)
are resolved and instantiated during the pre-pass.

**4. Uniform conditional visibility (`when:`) on all layout items**
Legacy `options` and `container.option` are superseded by `when: { <param_name>: <expected_value> }`,
supported uniformly on all layout items (`Container`, `Text`, `Qr`, `Image`, `Line`). An item is rendered
only if all conditions in its `when` map evaluate to true against resolved parameter values.

**5. Lazy missing-field evaluation**
Missing field evaluation (`422 MissingField`) occurs lazily as active layout items are measured and
rendered. If a required parameter without a default is only referenced in an inactive `when` branch, its
omission in the request payload does NOT produce an error.

**6. Maximum label dimension setting (`max_label_dimension_mm`)**
To guard against unbounded memory allocation and renderer denial of service from extreme dynamic lengths,
the application setting `max_label_dimension_mm` (default `1000.0` mm) is enforced in the render pre-pass.
Any resolved dimension that exceeds this limit, or is non-positive, returns `422 UnsupportedLayoutItem`
with reason `dimension_exceeds_limit`.

**7. Unified API and UI contracts**
- `POST /api/render/label`, `POST /api/batch`, `POST /api/import/csv`, and `POST /api/print` pass parameter
  inputs uniformly in `data` (with `fields` accepted as a synonym on `POST /api/print`).
- `GET /api/templates/{id}` exposes parameter definitions (`params`), enabling client-side forms (`Print`,
  `Import`, `Connect`) to auto-generate appropriate form controls (text inputs, textareas, sliders,
  toggles, dropdowns) pre-filled with parameter defaults.

## Consequences

- All template inputs share a single, consistent declaration syntax, eliminating the conceptual divide
  between layout options, content fields, and styling variables.
- Legacy `options` and `container.option` are completely replaced by `params:` and `when:`.
- The Web UI automatically renders type-appropriate controls and slider ranges without template-specific UI code.
- Layout geometry remains validated at load time using declared defaults, while supporting flexible
  runtime customization.
