use std::collections::{BTreeMap, HashMap};

use labeler::datetime_fmt::DateTimeResolver;
use labeler::models::*;
use labeler::parse::parse_template;
use labeler::render::*;
use labeler::templates::{TemplateContent, TemplateDefinition};

fn parse_and_validate(yaml: &str) -> Result<TemplateContent, labeler::errors::AppError> {
    let content = parse_template(yaml).map_err(|e| {
        labeler::errors::AppError::template_invalid(
            labeler::reason::Reason::TemplateParseFailed,
            e.to_string(),
        )
    })?;
    content.validate().map_err(|e| {
        labeler::errors::AppError::template_invalid(
            labeler::reason::Reason::TemplateValidationFailed,
            e,
        )
    })?;
    Ok(content)
}

#[test]
fn acceptance_templates_render_and_overflow_cases_fail_with_item_out_of_frame() {
    let dt_formats = BTreeMap::new();
    let dt = DateTimeResolver {
        formats: &dt_formats,
        now: chrono::Local::now(),
    };
    let vars = BTreeMap::new();
    let opts = ImageRenderOptions::default();

    // 1. A when:-gated column that closes its hole, rendered with the gate on and off
    let yaml_gated = r#"
name: Gated Column
unit: mm
dpi: 200
params:
  - name: show_middle
    type: enum
    values: ["yes", "no"]
    default: "yes"
format: { type: single, width: 60, height: 40 }
layout:
  - type: container
    at: [0, 0]
    size: [60, 40]
    padding: 2
    flow: { direction: column, gap: 3 }
    items:
      - type: text
        value: "First Line"
        size: [fill, 8]
        font_size: 8
      - type: text
        when: { show_middle: "yes" }
        value: "Middle Line"
        size: [fill, 8]
        font_size: 8
      - type: text
        value: "Third Line"
        size: [fill, 8]
        font_size: 8
"#;
    let t_gated = parse_and_validate(yaml_gated).unwrap();
    let mut data_on = HashMap::new();
    data_on.insert("show_middle".to_string(), serde_json::json!("yes"));
    let png_on = render_single_label_image(&t_gated, &data_on, &vars, &dt, opts).unwrap();

    let mut data_off = HashMap::new();
    data_off.insert("show_middle".to_string(), serde_json::json!("no"));
    let png_off = render_single_label_image(&t_gated, &data_off, &vars, &dt, opts).unwrap();
    assert_ne!(png_on, png_off);

    // 2. A row whose children are content-sized text, rendered with a short and a long value
    let yaml_dynamic_row = r#"
name: Dynamic Text Row
unit: mm
dpi: 200
params:
  - name: val
    type: string
    default: "Short"
format: { type: single, width: { min: 20, max: 150 }, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [content, 20]
    flow: { direction: row, gap: 4 }
    padding: 2
    items:
      - type: text
        value: "Prefix:"
        size: [content, 12]
        font_size: 10
      - type: text
        value: "{val}"
        size: [content, 12]
        font_size: 10
"#;
    let t_dyn_row = parse_and_validate(yaml_dynamic_row).unwrap();
    let mut data_short = HashMap::new();
    data_short.insert("val".to_string(), serde_json::json!("A"));
    let png_short = render_single_label_image(&t_dyn_row, &data_short, &vars, &dt, opts).unwrap();

    let mut data_long = HashMap::new();
    data_long.insert(
        "val".to_string(),
        serde_json::json!("A very long value expanding the row"),
    );
    let png_long = render_single_label_image(&t_dyn_row, &data_long, &vars, &dt, opts).unwrap();

    let img_short = image::load_from_memory(&png_short).unwrap();
    let img_long = image::load_from_memory(&png_long).unwrap();
    assert!(img_long.width() > img_short.width());

    // 3. A row whose middle child's value is empty, confirming one gap rather than two
    let yaml_empty_middle = r#"
name: Empty Middle
unit: mm
dpi: 200
params:
  - name: mid
    type: string
    default: ""
format: { type: single, width: 80, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [80, 20]
    flow: { direction: row, gap: 5 }
    items:
      - type: text
        value: "Left"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "{mid}"
        size: [content, 10]
        font_size: 8
      - type: text
        value: "Right"
        size: [20, 10]
        font_size: 8
"#;
    let t_empty_mid = parse_and_validate(yaml_empty_middle).unwrap();
    let mut data_empty = HashMap::new();
    data_empty.insert("mid".to_string(), serde_json::json!(""));
    let _png_empty =
        render_single_label_image(&t_empty_mid, &data_empty, &vars, &dt, opts).unwrap();

    // 4. Both nesting directions: flow inside absolute, absolute inside flow, flow inside flow
    let yaml_nesting = r#"
name: Nesting Test
unit: mm
dpi: 200
format: { type: single, width: 100, height: 50 }
layout:
  - type: container
    at: [5, 5]
    size: [90, 40]
    flow: { direction: row, gap: 5 }
    padding: 2
    items:
      - type: container
        size: [35, fill]
        items:
          - type: text
            value: "Abs in Flow"
            at: [2, 2]
            size: [30, 10]
            font_size: 8
      - type: container
        size: [40, fill]
        flow: { direction: column, gap: 2 }
        items:
          - type: text
            value: "Flow in Flow 1"
            size: [fill, 10]
            font_size: 8
          - type: text
            value: "Flow in Flow 2"
            size: [fill, 10]
            font_size: 8
"#;
    let t_nesting = parse_and_validate(yaml_nesting).unwrap();
    let _png_nesting =
        render_single_label_image(&t_nesting, &HashMap::new(), &vars, &dt, opts).unwrap();

    // 5. A dynamic-width label sized by a flow container, beside a non-flow content-width text item
    let yaml_dyn_flow = r#"
name: Dynamic Flow beside Non-flow
unit: mm
dpi: 200
format: { type: single, width: { min: 10, max: 120 }, height: 25 }
layout:
  - type: container
    at: [0, 0]
    size: [content, 20]
    flow: { direction: row, gap: 3 }
    padding: 1
    items:
      - type: text
        value: "Flow Text"
        size: [content, 10]
        font_size: 8
      - type: qr
        value: "ABC"
        size: [10, 10]
"#;
    let t_dyn_flow = parse_and_validate(yaml_dyn_flow).unwrap();
    let _png_dyn_flow =
        render_single_label_image(&t_dyn_flow, &HashMap::new(), &vars, &dt, opts).unwrap();

    // 6. A rotated flow container, confirming it packs in author space
    let yaml_rotated = r#"
name: Rotated Flow
unit: mm
dpi: 200
format: { type: single, width: 50, height: 80 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 80]
    rotate: 90
    flow: { direction: row, gap: 5 }
    padding: 2
    items:
      - type: text
        value: "Author Row Item 1"
        size: [30, 15]
        font_size: 8
      - type: text
        value: "Author Row Item 2"
        size: [30, 15]
        font_size: 8
"#;
    let t_rotated = parse_and_validate(yaml_rotated).unwrap();
    let _png_rotated =
        render_single_label_image(&t_rotated, &HashMap::new(), &vars, &dt, opts).unwrap();

    // 6a. A column whose children overrun the padded inner box: fails item_out_of_frame, not coord_out_of_frame
    let yaml_col_overrun = r#"
name: Column Overrun
unit: mm
dpi: 200
format: { type: single, width: 40, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [40, 20]
    flow: { direction: column, gap: 5 }
    items:
      - type: text
        value: "First"
        size: [20, 12]
        font_size: 8
      - type: text
        value: "Second"
        size: [20, 12]
        font_size: 8
"#;
    let t_col_overrun = parse_and_validate(yaml_col_overrun).unwrap();
    let err_col_overrun =
        render_single_label_image(&t_col_overrun, &HashMap::new(), &vars, &dt, opts).unwrap_err();
    assert_eq!(err_col_overrun.reason(), Some("item_out_of_frame"));
    assert!(err_col_overrun.message_text().contains("items[1]"));

    // 7. A fill packed child alone in its container, and the same child beside a sibling
    let yaml_fill_alone = r#"
name: Fill Alone
unit: mm
dpi: 200
format: { type: single, width: 60, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [60, 20]
    flow: { direction: row }
    items:
      - type: text
        value: "Fill Alone"
        size: [fill, 10]
        font_size: 8
"#;
    let t_fill_alone = parse_and_validate(yaml_fill_alone).unwrap();
    let _png_fill_alone =
        render_single_label_image(&t_fill_alone, &HashMap::new(), &vars, &dt, opts).unwrap();

    let yaml_fill_sibling = r#"
name: Fill Sibling
unit: mm
dpi: 200
format: { type: single, width: 60, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [60, 20]
    flow: { direction: row, gap: 5 }
    items:
      - type: text
        value: "Fixed"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "Uncapped Fill"
        size: [fill, 10]
        font_size: 8
"#;
    let t_fill_sibling = parse_and_validate(yaml_fill_sibling).unwrap();
    let err_fill_sibling =
        render_single_label_image(&t_fill_sibling, &HashMap::new(), &vars, &dt, opts).unwrap_err();
    assert_eq!(err_fill_sibling.reason(), Some("item_out_of_frame"));
    assert!(err_fill_sibling.message_text().contains("items[1]"));

    // 8. A flow container as the root of a sheet slot
    let yaml_sheet = r#"
name: Sheet Flow Slot
unit: mm
dpi: 200
format:
  type: sheet
  paper_width: 210
  paper_height: 297
  label_width: 60
  label_height: 30
  positions: [[10, 10], [80, 10]]
layout:
  - type: container
    at: [0, 0]
    size: [60, 30]
    flow: { direction: row, gap: 4 }
    padding: 2
    items:
      - type: text
        value: "Sheet Item"
        size: [20, 10]
        font_size: 8
      - type: qr
        value: "DATA"
        size: [10, 10]
"#;
    let t_sheet = parse_and_validate(yaml_sheet).unwrap();
    let t_sheet_def = TemplateDefinition {
        id: "sheet_test".to_string(),
        group: None,
        content: t_sheet,
    };
    let labels = vec![LabelInput {
        data: HashMap::new(),
    }];
    let settings = BTreeMap::new();
    let pdf_sheet = render_sheet_pages(&t_sheet_def, &labels, 0, &settings, &dt).unwrap();
    assert!(pdf_sheet.starts_with(b"%PDF"));

    // 9. A content-sized multiline text with a font_size range as a packed child
    let yaml_multiline_packed = r#"
name: Multiline Packed
unit: mm
dpi: 200
format: { type: single, width: 80, height: 30 }
layout:
  - type: container
    at: [0, 0]
    size: [80, 30]
    flow: { direction: row, gap: 5 }
    padding: 2
    items:
      - type: text
        value: "This is a multiline text item packed inside a flow container"
        size: [40, content]
        wrap: true
        font_size: { min: 6, max: 14 }
      - type: qr
        value: "MULTI"
        size: [15, 15]
"#;
    let t_multiline = parse_and_validate(yaml_multiline_packed).unwrap();
    let _png_multiline =
        render_single_label_image(&t_multiline, &HashMap::new(), &vars, &dt, opts).unwrap();
}

/// Proves headline flow layout contract rules: hole closure on gated children, single gaps
/// for empty values, frame rendering and error raising on zero-extent children, author-space
/// packing under rotation, element reordering, container fill collisions, and empty container padding.
#[test]
fn headline_flow_spec_scenarios_automated_assertions() {
    let dt_formats = BTreeMap::new();
    let dt = DateTimeResolver {
        formats: &dt_formats,
        now: chrono::Local::now(),
    };
    let vars = BTreeMap::new();
    let opts = ImageRenderOptions::default();

    // 1. A gated-off child leaves no hole
    let yaml_gated_row = r#"
name: Gated Row Hole Test
unit: mm
dpi: 200
params:
  - name: show_mid
    type: enum
    values: ["yes", "no"]
    default: "yes"
format: { type: single, width: { min: 10, max: 100 }, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [content, 20]
    flow: { direction: row, gap: 4 }
    items:
      - type: text
        value: "AAA"
        size: [20, 10]
        font_size: 8
      - type: text
        when: { show_mid: "yes" }
        value: "BBB"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "CCC"
        size: [20, 10]
        font_size: 8
"#;
    let t_gated = parse_and_validate(yaml_gated_row).unwrap();
    let mut data_on = HashMap::new();
    data_on.insert("show_mid".to_string(), serde_json::json!("yes"));
    let mut data_off = HashMap::new();
    data_off.insert("show_mid".to_string(), serde_json::json!("no"));

    let png_on = render_single_label_image(&t_gated, &data_on, &vars, &dt, opts).unwrap();
    let png_off = render_single_label_image(&t_gated, &data_off, &vars, &dt, opts).unwrap();
    let img_on = image::load_from_memory(&png_on).unwrap();
    let img_off = image::load_from_memory(&png_off).unwrap();
    // Gate on: 20 + 20 + 20 + 4 + 4 = 68mm; Gate off: 20 + 20 + 4 = 44mm
    let px_68mm = (68.0_f32 / 25.4 * 200.0).round() as u32;
    let px_44mm = (44.0_f32 / 25.4 * 200.0).round() as u32;
    assert_eq!(img_on.width(), px_68mm);
    assert_eq!(img_off.width(), px_44mm);

    // 2. An empty value leaves no double gap
    let yaml_empty_gap = r#"
name: Empty Value Gap Test
unit: mm
dpi: 200
params:
  - name: mid
    type: string
    default: ""
format: { type: single, width: { min: 10, max: 100 }, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [content, 20]
    flow: { direction: row, gap: 4 }
    items:
      - type: text
        value: "AAA"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "{mid}"
        size: [content, 10]
        font_size: 8
      - type: text
        value: "CCC"
        size: [20, 10]
        font_size: 8
"#;
    let t_empty_gap = parse_and_validate(yaml_empty_gap).unwrap();
    let mut data_mid_empty = HashMap::new();
    data_mid_empty.insert("mid".to_string(), serde_json::json!(""));
    let png_empty_gap =
        render_single_label_image(&t_empty_gap, &data_mid_empty, &vars, &dt, opts).unwrap();
    let img_empty_gap = image::load_from_memory(&png_empty_gap).unwrap();
    // Sized to exactly 20 + 20 + 4 = 44mm (one gap)
    assert_eq!(img_empty_gap.width(), px_44mm);

    // Trailing empty child in content-sized container renders without trailing gap error
    let yaml_trailing_empty = r#"
name: Trailing Empty Content
unit: mm
dpi: 200
params:
  - name: tail
    type: string
    default: ""
format: { type: single, width: { min: 10, max: 100 }, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [content, 20]
    flow: { direction: row, gap: 4 }
    items:
      - type: text
        value: "AAA"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "CCC"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "{tail}"
        size: [content, 10]
        font_size: 8
"#;
    let t_trailing = parse_and_validate(yaml_trailing_empty).unwrap();
    let png_trailing =
        render_single_label_image(&t_trailing, &HashMap::new(), &vars, &dt, opts).unwrap();
    let img_trailing = image::load_from_memory(&png_trailing).unwrap();
    assert_eq!(img_trailing.width(), px_44mm);

    // Trailing empty child in fixed-size container whose occupying children exactly fill it
    let yaml_trailing_fixed = r#"
name: Trailing Empty Fixed
unit: mm
dpi: 200
params:
  - name: tail
    type: string
    default: ""
format: { type: single, width: 44, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [44, 20]
    flow: { direction: row, gap: 4 }
    items:
      - type: text
        value: "AAA"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "CCC"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "{tail}"
        size: [content, 10]
        font_size: 8
"#;
    let t_trailing_fixed = parse_and_validate(yaml_trailing_fixed).unwrap();
    let png_trailing_fixed =
        render_single_label_image(&t_trailing_fixed, &HashMap::new(), &vars, &dt, opts).unwrap();
    let img_trailing_fixed = image::load_from_memory(&png_trailing_fixed).unwrap();
    assert_eq!(img_trailing_fixed.width(), px_44mm);

    // 3. A zero-extent child still draws its frame and still raises its errors
    let yaml_zero_frame = r#"
name: Zero Extent Frame
unit: mm
dpi: 200
format: { type: single, width: 40, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [40, 20]
    flow: { direction: row }
    items:
      - type: container
        size: [content, 10]
        stroke: { thickness: 0.5 }
        items: []
      - type: text
        value: "Next"
        size: [20, 10]
        font_size: 8
"#;
    let yaml_zero_no_frame = r#"
name: Zero Extent No Frame
unit: mm
dpi: 200
format: { type: single, width: 40, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [40, 20]
    flow: { direction: row }
    items:
      - type: container
        size: [content, 10]
        items: []
      - type: text
        value: "Next"
        size: [20, 10]
        font_size: 8
"#;
    let t_zero_frame = parse_and_validate(yaml_zero_frame).unwrap();
    let t_zero_no_frame = parse_and_validate(yaml_zero_no_frame).unwrap();
    let png_zero_frame =
        render_single_label_image(&t_zero_frame, &HashMap::new(), &vars, &dt, opts).unwrap();
    let png_zero_no_frame =
        render_single_label_image(&t_zero_no_frame, &HashMap::new(), &vars, &dt, opts).unwrap();
    assert_ne!(png_zero_frame, png_zero_no_frame);

    let yaml_zero_err = r#"
name: Zero Extent Error
unit: mm
dpi: 200
params:
  - name: h
    type: number
    default: 10
format: { type: single, width: 40, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [40, 20]
    flow: { direction: row }
    items:
      - type: container
        size: [content, "{h}"]
        items: []
      - type: text
        value: "Next"
        size: [20, 10]
        font_size: 8
"#;
    let t_zero_err = parse_and_validate(yaml_zero_err).unwrap();
    let mut data_zero_err = HashMap::new();
    data_zero_err.insert("h".to_string(), serde_json::json!(25));
    let err_zero =
        render_single_label_image(&t_zero_err, &data_zero_err, &vars, &dt, opts).unwrap_err();
    assert_eq!(err_zero.reason(), Some("item_out_of_frame"));
    assert!(err_zero.message_text().contains("items[0]"));

    // A text child interpolating a missing field fails with MissingField whether extent is zero or not
    let yaml_missing_param = r#"
name: Missing Field In Flow
unit: mm
dpi: 200
params:
  - name: unsupplied_variable
    type: string
format: { type: single, width: 40, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [40, 20]
    flow: { direction: row }
    items:
      - type: text
        value: "{unsupplied_variable}"
        size: [content, 10]
        font_size: 8
"#;
    let t_missing = parse_and_validate(yaml_missing_param).unwrap();
    let err_missing =
        render_single_label_image(&t_missing, &HashMap::new(), &vars, &dt, opts).unwrap_err();
    assert_eq!(err_missing.code(), "MissingField");

    // 4. A quarter turn packs in author space
    let yaml_turn_flow = r#"
name: Quarter Turn Flow
unit: mm
dpi: 200
format: { type: single, width: 30, height: 60 }
layout:
  - type: container
    at: [0, 0]
    size: [30, 60]
    rotate: 90
    flow: { direction: row, gap: 5 }
    padding: 2
    items:
      - type: text
        value: "Item 1"
        size: [25, 10]
        font_size: 8
      - type: text
        value: "Item 2"
        size: [25, 10]
        font_size: 8
"#;
    let yaml_turn_abs = r#"
name: Quarter Turn Abs
unit: mm
dpi: 200
format: { type: single, width: 30, height: 60 }
layout:
  - type: container
    at: [0, 0]
    size: [30, 60]
    rotate: 90
    padding: 2
    items:
      - type: text
        value: "Item 1"
        at: [0, 16]
        size: [25, 10]
        font_size: 8
      - type: text
        value: "Item 2"
        at: [30, 16]
        size: [25, 10]
        font_size: 8
"#;
    let t_turn_flow = parse_and_validate(yaml_turn_flow).unwrap();
    let t_turn_abs = parse_and_validate(yaml_turn_abs).unwrap();
    let png_turn_flow =
        render_single_label_image(&t_turn_flow, &HashMap::new(), &vars, &dt, opts).unwrap();
    let png_turn_abs =
        render_single_label_image(&t_turn_abs, &HashMap::new(), &vars, &dt, opts).unwrap();
    assert_eq!(png_turn_flow, png_turn_abs);

    // 5. Reordering packed children reorders the label
    let yaml_order_ab = r#"
name: AB
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    flow: { direction: row, gap: 5 }
    items:
      - type: text
        value: "AAA"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "BBB"
        size: [20, 10]
        font_size: 8
"#;
    let yaml_order_ba = r#"
name: BA
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    flow: { direction: row, gap: 5 }
    items:
      - type: text
        value: "BBB"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "AAA"
        size: [20, 10]
        font_size: 8
"#;
    let t_ab = parse_and_validate(yaml_order_ab).unwrap();
    let t_ba = parse_and_validate(yaml_order_ba).unwrap();
    let png_ab = render_single_label_image(&t_ab, &HashMap::new(), &vars, &dt, opts).unwrap();
    let png_ba = render_single_label_image(&t_ba, &HashMap::new(), &vars, &dt, opts).unwrap();
    assert_ne!(png_ab, png_ba);

    // 6. A packed container with no size fills, and two of them collide
    let yaml_fill_collide = r#"
name: Fill Collide
unit: mm
dpi: 200
format: { type: single, width: 60, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [60, 20]
    flow: { direction: row }
    items:
      - type: container
        items:
          - type: text
            value: "First"
            size: [20, 10]
            font_size: 8
      - type: container
        items:
          - type: text
            value: "Second"
            size: [20, 10]
            font_size: 8
"#;
    let t_fill_collide = parse_and_validate(yaml_fill_collide).unwrap();
    let err_collide =
        render_single_label_image(&t_fill_collide, &HashMap::new(), &vars, &dt, opts).unwrap_err();
    assert_eq!(err_collide.reason(), Some("item_out_of_frame"));
    assert!(err_collide.message_text().contains("items[1]"));

    // The same two containers spelling size: [content, content] pack side by side and render
    let yaml_content_side_by_side = r#"
name: Content Side By Side
unit: mm
dpi: 200
format: { type: single, width: 60, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [60, 20]
    flow: { direction: row, gap: 5 }
    items:
      - type: container
        size: [content, content]
        items:
          - type: text
            value: "First"
            size: [20, 10]
            font_size: 8
      - type: container
        size: [content, content]
        items:
          - type: text
            value: "Second"
            size: [20, 10]
            font_size: 8
"#;
    let t_side_by_side = parse_and_validate(yaml_content_side_by_side).unwrap();
    let _png_side_by_side =
        render_single_label_image(&t_side_by_side, &HashMap::new(), &vars, &dt, opts).unwrap();

    // 7. Every child gated off leaves a padding-sized container
    let yaml_all_gated = r#"
name: All Gated
unit: mm
dpi: 200
params:
  - name: show
    type: enum
    values: ["yes", "no"]
    default: "no"
format: { type: single, width: { min: 10, max: 100 }, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [content, 20]
    padding: 3
    flow: { direction: row, gap: 4 }
    items:
      - type: text
        when: { show: "yes" }
        value: "Hidden"
        size: [20, 10]
        font_size: 8
"#;
    let t_all_gated = parse_and_validate(yaml_all_gated).unwrap();
    let png_all_gated =
        render_single_label_image(&t_all_gated, &HashMap::new(), &vars, &dt, opts).unwrap();
    let img_all_gated = image::load_from_memory(&png_all_gated).unwrap();
    // width clamped to format.width.min (10mm)
    let px_10mm = (10.0_f32 / 25.4 * 200.0).round() as u32;
    assert_eq!(img_all_gated.width(), px_10mm);

    // Multiline content-sized text placed after a QR code overruns and fails item_out_of_frame
    let yaml_multiline_after_qr = r#"
name: Multiline After QR
unit: mm
dpi: 200
format: { type: single, width: 70, height: 30 }
layout:
  - type: container
    at: [0, 0]
    size: [70, 30]
    flow: { direction: row, gap: 2 }
    items:
      - type: qr
        value: "CODE"
        size: [26, 26]
      - type: text
        value: "This multiline text wraps to the full container width and therefore overruns when placed second"
        size: [content, 26]
        wrap: true
        font_size: 8
"#;
    let t_multiline_overrun = parse_and_validate(yaml_multiline_after_qr).unwrap();
    let err_multiline =
        render_single_label_image(&t_multiline_overrun, &HashMap::new(), &vars, &dt, opts)
            .unwrap_err();
    assert_eq!(err_multiline.reason(), Some("item_out_of_frame"));
    assert!(err_multiline.message_text().contains("items[1]"));
}
