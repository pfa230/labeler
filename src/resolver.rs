use std::collections::HashMap;

use crate::models::{
    DynamicValue, Extent, Flow, FlowDirection, Padding, Placement, Rotation, SizeValue,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    /// Non-negative coordinate `v` (e.g. 0.0, 10.0). Low edge is at `v`.
    Plain(f32),
    /// Sign-negative coordinate with inset `a = -v` (e.g. -0.0 has inset 0.0, -2.0 has inset 2.0).
    /// Low edge is at `frame - a`.
    EdgeRelative(f32),
    /// An item with no anchor (a packed child).
    Absent,
}

impl Anchor {
    pub fn is_edge_relative(&self) -> bool {
        matches!(self, Anchor::EdgeRelative(_))
    }

    pub fn inset(&self) -> Option<f32> {
        match self {
            Anchor::Plain(_) | Anchor::Absent => None,
            Anchor::EdgeRelative(a) => Some(*a),
        }
    }

    pub fn resolve(&self, frame: f32) -> f32 {
        match self {
            Anchor::Plain(v) => *v,
            Anchor::EdgeRelative(a) => frame - *a,
            Anchor::Absent => panic!("Anchor::Absent has no coordinate"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExtentSource {
    /// An authored extent with known magnitude (number, parameter value, or constant-`to`).
    Author(f32),
    /// A shrinking `to` (at sign-negative with inset a, to non-negative with value to_val).
    /// Permitted only on a resolved axis. Evaluates on frame F to `to_val + a - F`.
    ShrinkingTo { to_val: f32, inset_a: f32 },
    /// Content extent: intrinsic size of the item.
    Content,
    /// Frame extent: stretches to the available frame extent.
    Frame,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisSpec {
    pub source: ExtentSource,
    pub anchor: Anchor,
    /// The reserved far-edge margin from `to` if sign-negative with inset `b = -to_val`, else 0.0.
    pub inset: f32,
    /// Whether the author wrote this extent as a `to` corner rather than a `size`. Recorded by the
    /// classifier so no later rule has to look at [`Extent`] again to know which spelling produced
    /// the extent it is judging.
    pub written_as_to: bool,
}

impl AxisSpec {
    pub fn is_shrinking_to(&self) -> bool {
        matches!(self.source, ExtentSource::ShrinkingTo { .. })
    }

    pub fn cap_binds(&self) -> bool {
        matches!(self.source, ExtentSource::Content | ExtentSource::Frame)
    }

    pub fn demands_intrinsic(&self) -> bool {
        matches!(self.source, ExtentSource::Content | ExtentSource::Frame)
    }
}

/// Classify an item's extent on `axis` (0 = width/x, 1 = height/y) into an [`AxisSpec`].
/// This is the single semantic inspection of [`SizeValue`] and [`Extent`].
pub fn source_of(
    placement: &Placement,
    axis: usize,
    geometry_values: &HashMap<String, f32>,
) -> AxisSpec {
    let anchor = match &placement.at {
        Some(pos) => {
            let raw = pos.0[axis];
            if raw.is_sign_negative() {
                Anchor::EdgeRelative(-raw)
            } else {
                Anchor::Plain(raw)
            }
        }
        None => Anchor::Absent,
    };

    match &placement.extent {
        Extent::Size(size) => {
            let sv = &size.0[axis];
            let source = match sv {
                SizeValue::Content => ExtentSource::Content,
                SizeValue::Fill => ExtentSource::Frame,
                SizeValue::Dynamic(DynamicValue::Literal(v)) => ExtentSource::Author(*v),
                SizeValue::Dynamic(DynamicValue::Ref(ref_name)) => {
                    let v = geometry_values
                        .get(ref_name)
                        .copied()
                        .expect("validated parameter must have a value in geometry_values");
                    ExtentSource::Author(v)
                }
            };
            AxisSpec {
                source,
                anchor,
                inset: 0.0,
                written_as_to: false,
            }
        }
        Extent::To(to_pos) => {
            // Like `Anchor::resolve`'s deliberate panic on `Anchor::Absent`, an `Extent::To` on a placement
            // with no anchor reaching `source_of` is an invariant violation, not a coordinate. `convert.rs`
            // refuses `to` on packed (anchorless) children at template load time, making this unreachable
            // in valid execution; we panic with an explicit invariant message rather than returning a silent default.
            let at_pos = placement.at.as_ref().unwrap_or_else(|| {
                panic!("invariant violation: Extent::To placement must have an anchor: a packed child is anchorless and cannot carry `to:`");
            });
            let at_raw = at_pos.0[axis];
            let at_sign_neg = at_raw.is_sign_negative();
            let a = if at_sign_neg { -at_raw } else { 0.0 };
            let to_raw = to_pos.0[axis];
            let to_sign_neg = to_raw.is_sign_negative();
            let b = if to_sign_neg { -to_raw } else { 0.0 };
            let inset = if to_sign_neg { b } else { 0.0 };

            let source = match (at_sign_neg, to_sign_neg) {
                // Both corners non-negative (slope 0, constant authored extent)
                (false, false) => ExtentSource::Author(to_raw - at_raw),
                // Both corners sign-negative (slope 0, constant authored extent)
                (true, true) => ExtentSource::Author(a - b),
                // at non-negative, to sign-negative (slope +1, stretches with frame)
                (false, true) => ExtentSource::Frame,
                // at sign-negative, to non-negative (slope -1, shrinks as frame grows)
                (true, false) => ExtentSource::ShrinkingTo {
                    to_val: to_raw,
                    inset_a: a,
                },
            };

            AxisSpec {
                source,
                anchor,
                inset,
                written_as_to: true,
            }
        }
    }
}

/// The available extent an item has from its anchor in `frame`.
pub fn available(frame: f32, axis_spec: &AxisSpec) -> f32 {
    match axis_spec.anchor {
        Anchor::Plain(at) => frame - at - axis_spec.inset,
        Anchor::EdgeRelative(a) => a - axis_spec.inset,
        Anchor::Absent => frame,
    }
}

/// Resolve the item's concrete extent from its classified axis, concrete frame, available extent,
/// optional cap, and optional intrinsic size.
pub fn resolve(
    axis_spec: &AxisSpec,
    frame: f32,
    available: f32,
    cap: Option<f32>,
    intrinsic: Option<f32>,
) -> f32 {
    match axis_spec.source {
        ExtentSource::Author(val) => val,
        ExtentSource::ShrinkingTo { to_val, inset_a } => to_val + inset_a - frame,
        ExtentSource::Content => {
            let mut ext = intrinsic.unwrap_or(0.0).min(available);
            if let Some(c) = cap {
                ext = ext.min(c);
            }
            ext
        }
        ExtentSource::Frame => {
            let mut ext = available;
            if let Some(c) = cap {
                ext = ext.min(c);
            }
            ext
        }
    }
}

/// The claim an item makes for reporting upward into its parent frame requirement.
pub fn claim(
    axis_spec: &AxisSpec,
    frame: f32,
    available: f32,
    cap: Option<f32>,
    intrinsic: Option<f32>,
) -> f32 {
    match axis_spec.source {
        ExtentSource::Author(val) => val,
        ExtentSource::ShrinkingTo { to_val, inset_a } => to_val + inset_a - frame,
        ExtentSource::Content | ExtentSource::Frame => {
            let mut ext = intrinsic.unwrap_or(0.0).min(available).max(0.0);
            if let Some(c) = cap {
                ext = ext.min(c);
            }
            ext
        }
    }
}

/// The frame requirement imposed on `frame` by an item with the given classified axis and claim.
pub fn requirement(axis_spec: &AxisSpec, claim: f32) -> f32 {
    match (axis_spec.anchor, axis_spec.source) {
        (Anchor::Plain(at), ExtentSource::Author(_)) => at + claim,
        (Anchor::EdgeRelative(a), ExtentSource::Author(_)) => a,
        (Anchor::EdgeRelative(a), ExtentSource::ShrinkingTo { to_val, .. }) => a.max(to_val),
        (Anchor::Plain(_), ExtentSource::ShrinkingTo { .. }) => unreachable!(),
        (Anchor::Plain(at), ExtentSource::Content) => at + claim,
        (Anchor::EdgeRelative(a), ExtentSource::Content) => a,
        (Anchor::Plain(at), ExtentSource::Frame) => at + claim + axis_spec.inset,
        (Anchor::EdgeRelative(a), ExtentSource::Frame) => a,
        (Anchor::Absent, _) => claim,
    }
}

/// A coordinate's requirement on its frame.
pub fn coord_requirement(coord: f32) -> f32 {
    if coord.is_sign_negative() {
        -coord
    } else {
        coord
    }
}

/// A line's requirement on its frame for the given axis.
pub fn line_axis_requirement(at_coord: f32, to_coord: f32) -> f32 {
    coord_requirement(at_coord).max(coord_requirement(to_coord))
}

/// Compute inner resolved-axis state for a container.
pub fn container_inner_axes_resolved(
    placement: &Placement,
    parent_axes_resolved: [bool; 2],
    rotation: Rotation,
    geometry_values: &HashMap<String, f32>,
) -> [bool; 2] {
    let mut resolved = [false, false];
    for axis in 0..2 {
        let spec = source_of(placement, axis, geometry_values);
        let axis_res = match spec.source {
            ExtentSource::Author(_) | ExtentSource::ShrinkingTo { .. } => true,
            ExtentSource::Content => false,
            ExtentSource::Frame => {
                if spec.anchor.is_edge_relative() {
                    true
                } else {
                    parent_axes_resolved[axis]
                }
            }
        };
        resolved[axis] = axis_res;
    }
    if rotation.swaps_axes() {
        [resolved[1], resolved[0]]
    } else {
        resolved
    }
}

/// The extent a stage that has not measured resolves an item to. Availability stands in for the
/// intrinsic, which by [`resolve`]'s `content` arm makes `content` resolve exactly as `fill` does.
/// Load uses it because it cannot measure; render's measuring walk uses it for the same reason,
/// since an item's children are measured against its box before that box's own intrinsic exists.
/// This is the one place that substitution is spelled.
pub fn resolve_unmeasured(axis_spec: &AxisSpec, frame: f32, cap: Option<f32>) -> f32 {
    let avail = available(frame, axis_spec);
    resolve(axis_spec, frame, avail, cap, Some(avail))
}

/// The tolerance every bounds comparison uses, so load and render agree on the edge cases.
pub const BOUNDS_EPSILON: f32 = 1.0e-4;

/// A resolved box in its frame: low corner and extents, in the template `unit`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// What a placement can be wrong about once its axes are classified. The rule lives here; the
/// vocabulary it is reported in does not, so load and render map the same violation into their own
/// message or reason without either of them owning a copy of the rule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Violation {
    /// The anchor resolves before the frame's origin.
    AnchorBeforeFrame { axis: usize },
    /// The anchor resolves past the frame's far edge.
    AnchorBeyondFrame { axis: usize },
    /// An authored extent is zero or negative where it is written. `written_as_to` carries the
    /// spelling the classifier saw, so a caller wording the refusal never re-reads the model.
    AuthoredExtentNotPositive { axis: usize, written_as_to: bool },
    /// A `to` corner is not above and to the right of its `at` once resolved.
    ExtentInverted { axis: usize },
    /// A resolved extent is negative and did not come from a `to`.
    ExtentNegative { axis: usize },
    /// The box runs past the frame's far edge.
    ExtentBeyondFrame { axis: usize },
}

impl Violation {
    pub fn axis(&self) -> usize {
        match self {
            Violation::AnchorBeforeFrame { axis }
            | Violation::AnchorBeyondFrame { axis }
            | Violation::AuthoredExtentNotPositive { axis, .. }
            | Violation::ExtentInverted { axis }
            | Violation::ExtentNegative { axis }
            | Violation::ExtentBeyondFrame { axis } => *axis,
        }
    }
}

fn frame_axis(frame: (f32, f32), axis: usize) -> f32 {
    if axis == 0 {
        frame.0
    } else {
        frame.1
    }
}

/// The far-edge bounds comparison checking whether a span `[low, low + extent]` ends within `limit`.
pub fn fits_frame(axis: usize, low: f32, extent: f32, limit: f32) -> Result<(), Violation> {
    if low > limit + BOUNDS_EPSILON {
        return Err(Violation::AnchorBeyondFrame { axis });
    }
    if low + extent > limit + BOUNDS_EPSILON {
        return Err(Violation::ExtentBeyondFrame { axis });
    }
    Ok(())
}

/// The rules that hold whatever an item measures to: where its anchor lands, and whether an extent
/// it wrote down is a positive length. A stage that has not measured yet applies them in the same
/// order [`place`] does, so the first thing wrong with a placement is reported the same way at both.
pub fn precheck(
    placement: &Placement,
    frame: Option<(f32, f32)>,
    geometry_values: &HashMap<String, f32>,
) -> Result<(), Violation> {
    if let Some(frame) = frame {
        for axis in 0..2 {
            let spec = source_of(placement, axis, geometry_values);
            if spec.anchor.resolve(frame_axis(frame, axis)) < -BOUNDS_EPSILON {
                return Err(Violation::AnchorBeforeFrame { axis });
            }
        }
    }
    for axis in 0..2 {
        let spec = source_of(placement, axis, geometry_values);
        if let ExtentSource::Author(val) = spec.source {
            if val <= 0.0 {
                return Err(Violation::AuthoredExtentNotPositive {
                    axis,
                    written_as_to: spec.written_as_to,
                });
            }
        }
    }
    // A `to` inverts on the frame alone: none of the three sources it can classify to consults an
    // intrinsic, so the extent here is the one [`place`] resolves later. Judging it before the item
    // measures is what keeps an inverted box from reaching the content, which would otherwise
    // report what it failed to do inside a negative box instead of the box being wrong.
    if let Some(frame) = frame {
        for axis in 0..2 {
            let spec = source_of(placement, axis, geometry_values);
            if !spec.written_as_to {
                continue;
            }
            let cap = if axis == 0 {
                placement.max_w
            } else {
                placement.max_h
            };
            if resolve_unmeasured(&spec, frame_axis(frame, axis), cap) <= 0.0 {
                return Err(Violation::ExtentInverted { axis });
            }
        }
    }
    Ok(())
}

/// Resolve a placement into its box and check it against its frame. `intrinsic` is data: a stage
/// that measured an axis passes what it measured, and `None` means this stage did not, which is
/// either because the axis does not demand an intrinsic — [`resolve`] then ignores it — or because
/// the stage cannot measure, in which case [`resolve_unmeasured`] supplies availability.
pub fn place(
    placement: &Placement,
    frame: (f32, f32),
    geometry_values: &HashMap<String, f32>,
    intrinsic: [Option<f32>; 2],
) -> Result<Placed, Violation> {
    precheck(placement, Some(frame), geometry_values)?;

    let spec_0 = source_of(placement, 0, geometry_values);
    let spec_1 = source_of(placement, 1, geometry_values);

    let w = match intrinsic[0] {
        Some(measured) => resolve(
            &spec_0,
            frame.0,
            available(frame.0, &spec_0),
            placement.max_w,
            Some(measured),
        ),
        None => resolve_unmeasured(&spec_0, frame.0, placement.max_w),
    };
    let h = match intrinsic[1] {
        Some(measured) => resolve(
            &spec_1,
            frame.1,
            available(frame.1, &spec_1),
            placement.max_h,
            Some(measured),
        ),
        None => resolve_unmeasured(&spec_1, frame.1, placement.max_h),
    };

    // Inversion belongs to `precheck`, which ran above: a `to` resolves without an intrinsic, so
    // the extent it judged is the one resolved here. What is left is the `size` spelling, whose
    // `content` and `fill` sources can only turn negative through availability.
    for (axis, extent) in [(0usize, w), (1usize, h)] {
        if extent < -BOUNDS_EPSILON {
            return Err(Violation::ExtentNegative { axis });
        }
    }

    let x = spec_0.anchor.resolve(frame.0);
    let y = spec_1.anchor.resolve(frame.1);
    fits_frame(0, x, w, frame.0)?;
    fits_frame(1, y, h, frame.1)?;

    Ok(Placed {
        x,
        y,
        w: w.max(0.0),
        h: h.max(0.0),
    })
}

/// Resolve a packed child's extents against its padded inner box and verify it fits.
pub fn resolve_packed(
    placement: &Placement,
    inner: (f32, f32),
    geometry_values: &HashMap<String, f32>,
    intrinsic: [Option<f32>; 2],
) -> Result<(f32, f32), Violation> {
    for axis in 0..2 {
        let spec = source_of(placement, axis, geometry_values);
        if let ExtentSource::Author(val) = spec.source {
            if val <= 0.0 {
                return Err(Violation::AuthoredExtentNotPositive {
                    axis,
                    written_as_to: spec.written_as_to,
                });
            }
        }
    }

    let spec_0 = source_of(placement, 0, geometry_values);
    let spec_1 = source_of(placement, 1, geometry_values);

    let w = match intrinsic[0] {
        Some(measured) => resolve(
            &spec_0,
            inner.0,
            available(inner.0, &spec_0),
            placement.max_w,
            Some(measured),
        ),
        None => resolve_unmeasured(&spec_0, inner.0, placement.max_w),
    };
    let h = match intrinsic[1] {
        Some(measured) => resolve(
            &spec_1,
            inner.1,
            available(inner.1, &spec_1),
            placement.max_h,
            Some(measured),
        ),
        None => resolve_unmeasured(&spec_1, inner.1, placement.max_h),
    };

    for (axis, extent) in [(0usize, w), (1usize, h)] {
        if extent < -BOUNDS_EPSILON {
            return Err(Violation::ExtentNegative { axis });
        }
    }

    fits_frame(0, 0.0, w, inner.0)?;
    fits_frame(1, 0.0, h, inner.1)?;

    Ok((w.max(0.0), h.max(0.0)))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowChildInput {
    pub resolved_box: (f32, f32),
    pub requirement: (f32, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowResult {
    pub rects: Vec<Placed>,
    pub assembled: (f32, f32),
}

/// Compute the flow arrangement for packed children inside a padded inner box.
/// Check accumulation in packing coordinates, then convert to drawing coordinates.
pub fn arrange_flow(
    inner: (f32, f32),
    flow: &Flow,
    children: &[FlowChildInput],
) -> Result<FlowResult, (usize, Violation)> {
    let is_row = matches!(flow.direction, FlowDirection::Row);
    let primary_axis = if is_row { 0 } else { 1 };
    let inner_primary = if is_row { inner.0 } else { inner.1 };

    let n = children.len();
    let mut has_occupying_after = vec![false; n];
    let mut occ_after = false;
    for i in (0..n).rev() {
        has_occupying_after[i] = occ_after;
        let ext_p = if is_row {
            children[i].resolved_box.0
        } else {
            children[i].resolved_box.1
        };
        if ext_p > 0.0 {
            occ_after = true;
        }
    }

    let mut cursor = 0.0_f32;
    let mut is_first_occupying = true;
    let mut num_occupying = 0usize;
    let mut sum_occupying_req_primary = 0.0_f32;
    let mut max_active_req_secondary = 0.0_f32;

    let mut rects = Vec::with_capacity(children.len());

    for (idx, child) in children.iter().enumerate() {
        let ext_p = if is_row {
            child.resolved_box.0
        } else {
            child.resolved_box.1
        };
        let ext_s = if is_row {
            child.resolved_box.1
        } else {
            child.resolved_box.0
        };
        let req_p = if is_row {
            child.requirement.0
        } else {
            child.requirement.1
        };
        let req_s = if is_row {
            child.requirement.1
        } else {
            child.requirement.0
        };

        max_active_req_secondary = max_active_req_secondary.max(req_s);

        let occupies = ext_p > 0.0;

        let child_lead_p = if occupies {
            if !is_first_occupying {
                cursor += flow.gap;
            }
            is_first_occupying = false;

            fits_frame(primary_axis, cursor, ext_p, inner_primary).map_err(|v| (idx, v))?;

            let lead = cursor;
            cursor += ext_p;
            num_occupying += 1;
            sum_occupying_req_primary += req_p;
            lead
        } else {
            let lead = if !is_first_occupying && has_occupying_after[idx] {
                cursor + flow.gap
            } else {
                cursor
            };
            fits_frame(primary_axis, lead, 0.0, inner_primary).map_err(|v| (idx, v))?;
            lead
        };

        let placed = if is_row {
            Placed {
                x: child_lead_p,
                y: inner.1 - ext_s,
                w: ext_p.max(0.0),
                h: ext_s.max(0.0),
            }
        } else {
            Placed {
                x: 0.0,
                y: inner.1 - (child_lead_p + ext_p),
                w: ext_s.max(0.0),
                h: ext_p.max(0.0),
            }
        };

        rects.push(placed);
    }

    let assembled_primary = if num_occupying == 0 {
        0.0
    } else {
        sum_occupying_req_primary + (num_occupying - 1) as f32 * flow.gap
    };
    let assembled_secondary = max_active_req_secondary;

    let assembled = if is_row {
        (assembled_primary, assembled_secondary)
    } else {
        (assembled_secondary, assembled_primary)
    };

    Ok(FlowResult { rects, assembled })
}

/// A container's outer box, the canvas its children are laid out on once rotation has swapped the
/// axes, the padded frame inside that canvas, and which of the child frame's axes are resolved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContainerGeometry {
    pub outer: (f32, f32),
    pub canvas: (f32, f32),
    pub inner: (f32, f32),
    pub child_axes_resolved: [bool; 2],
}

/// The canvas and padded inner frame an outer box gives its children.
pub fn container_frames(
    outer: (f32, f32),
    rotation: Rotation,
    padding: &Padding,
) -> ((f32, f32), (f32, f32)) {
    let canvas = if rotation.swaps_axes() {
        (outer.1, outer.0)
    } else {
        outer
    };
    let inner = (
        (canvas.0 - padding.left - padding.right).max(0.0),
        (canvas.1 - padding.top - padding.bottom).max(0.0),
    );
    (canvas, inner)
}

/// A container's whole geometry, from its classified axes down to the frame its children see.
/// Both stages call this rather than repeating the resolve, the rotation swap and the padding.
pub fn container_geometry(
    placement: &Placement,
    padding: &Padding,
    frame: (f32, f32),
    parent_axes_resolved: [bool; 2],
    geometry_values: &HashMap<String, f32>,
) -> ContainerGeometry {
    let rotation = rotation_of(placement);
    let spec_0 = source_of(placement, 0, geometry_values);
    let spec_1 = source_of(placement, 1, geometry_values);
    let outer = (
        resolve_unmeasured(&spec_0, frame.0, placement.max_w),
        resolve_unmeasured(&spec_1, frame.1, placement.max_h),
    );
    let (canvas, inner) = container_frames(outer, rotation, padding);
    ContainerGeometry {
        outer,
        canvas,
        inner,
        child_axes_resolved: container_inner_axes_resolved(
            placement,
            parent_axes_resolved,
            rotation,
            geometry_values,
        ),
    }
}

/// The orthogonal rotation a placement carries, defaulting to none.
pub fn rotation_of(placement: &Placement) -> Rotation {
    placement
        .rotate
        .and_then(Rotation::from_degrees)
        .unwrap_or(Rotation::R0)
}

/// The cap an axis is bound by, if the placement wrote one.
pub fn cap(placement: &Placement, axis: usize) -> Option<f32> {
    if axis == 0 {
        placement.max_w
    } else {
        placement.max_h
    }
}

/// What an item requires of its frame on `axis`: classify, take availability, form the claim, and
/// turn the claim into a requirement. Callers pass what they measured, or `None` for an axis they
/// did not measure.
pub fn axis_requirement(
    placement: &Placement,
    axis: usize,
    frame_extent: f32,
    geometry_values: &HashMap<String, f32>,
    intrinsic: Option<f32>,
) -> f32 {
    let spec = source_of(placement, axis, geometry_values);
    let avail = available(frame_extent, &spec);
    let claimed = claim(&spec, frame_extent, avail, cap(placement, axis), intrinsic);
    requirement(&spec, claimed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Position, Rotation, Size, SizeValue};

    fn to_placement(at: [f32; 2], to: [f32; 2]) -> Placement {
        Placement {
            at: Some(Position(at)),
            extent: Extent::To(Position(to)),
            max_w: None,
            max_h: None,
            rotate: None,
        }
    }

    /// The `to` orientation split is the subtlest rule here and two drafts got it wrong, so all four
    /// corner sign combinations are classified and resolved in one place.
    #[test]
    fn source_of_classifies_the_four_to_corner_combinations() {
        let geo = HashMap::new();
        let frame = 100.0;

        // Both corners plain: a constant the author wrote as two edges.
        let spec = source_of(&to_placement([10.0, 0.0], [40.0, 10.0]), 0, &geo);
        assert_eq!(spec.source, ExtentSource::Author(30.0));
        assert_eq!(spec.anchor, Anchor::Plain(10.0));
        assert_eq!(spec.inset, 0.0);
        assert!(spec.written_as_to);
        assert_eq!(
            resolve(&spec, frame, available(frame, &spec), None, None),
            30.0
        );

        // Both corners edge-relative: the two frame terms cancel, so it is a constant too.
        let spec = source_of(&to_placement([-10.0, 0.0], [-2.0, 10.0]), 0, &geo);
        assert_eq!(spec.source, ExtentSource::Author(8.0));
        assert_eq!(spec.anchor, Anchor::EdgeRelative(10.0));
        assert_eq!(spec.inset, 2.0);
        assert_eq!(
            resolve(&spec, frame, available(frame, &spec), None, None),
            8.0
        );
        assert_eq!(requirement(&spec, 8.0), 10.0);

        // Plain `at`, edge-relative `to`: one frame term survives, so it stretches.
        let spec = source_of(&to_placement([10.0, 0.0], [-2.0, 10.0]), 0, &geo);
        assert_eq!(spec.source, ExtentSource::Frame);
        assert_eq!(spec.inset, 2.0);
        assert_eq!(available(frame, &spec), 88.0);
        assert_eq!(
            resolve(&spec, frame, available(frame, &spec), None, None),
            88.0
        );
        assert_eq!(requirement(&spec, 88.0), 100.0);

        // Edge-relative `at`, plain `to`: the extent shrinks as the frame grows.
        let spec = source_of(&to_placement([-10.0, 0.0], [40.0, 10.0]), 0, &geo);
        assert_eq!(
            spec.source,
            ExtentSource::ShrinkingTo {
                to_val: 40.0,
                inset_a: 10.0
            }
        );
        assert!(spec.is_shrinking_to());
        assert_eq!(
            resolve(&spec, 45.0, available(45.0, &spec), None, None),
            5.0
        );
        assert_eq!(requirement(&spec, 5.0), 40.0);
    }

    /// A cap binds a chosen extent and is inert on one the author wrote, whichever way it was
    /// spelled.
    #[test]
    fn a_cap_binds_only_a_chosen_extent() {
        let geo = HashMap::new();
        let stretching = source_of(&to_placement([0.0, 0.0], [-0.0, 10.0]), 0, &geo);
        assert!(stretching.cap_binds());
        assert_eq!(
            resolve(&stretching, 100.0, 100.0, Some(30.0), None),
            30.0,
            "a stretching `to` is capped"
        );

        let shrinking = source_of(&to_placement([-10.0, 0.0], [40.0, 10.0]), 0, &geo);
        assert!(!shrinking.cap_binds());
        assert_eq!(
            resolve(
                &shrinking,
                45.0,
                available(45.0, &shrinking),
                Some(2.0),
                None
            ),
            5.0,
            "a cap is inert on an authored shrinking `to`"
        );
    }

    /// The resolved-axis state is composed in author space and swapped by the rotation, so a
    /// quarter turn in either direction moves the same pair.
    #[test]
    fn the_resolved_axis_state_swaps_in_both_rotation_directions() {
        let geo = HashMap::new();
        let placement = Placement::sized(
            Position([0.0, 0.0]),
            Size([SizeValue::fill(), SizeValue::fixed(20.0)]),
        );
        let parent = [false, true];

        assert_eq!(
            container_inner_axes_resolved(&placement, parent, Rotation::R0, &geo),
            [false, true]
        );
        assert_eq!(
            container_inner_axes_resolved(&placement, parent, Rotation::R180, &geo),
            [false, true],
            "a half turn does not swap the axes"
        );
        for rotation in [Rotation::R90, Rotation::R270] {
            assert_eq!(
                container_inner_axes_resolved(&placement, parent, rotation, &geo),
                [true, false],
                "{rotation:?} must swap the pair"
            );
        }
    }

    /// Proves that `Anchor::Absent` resolves `available` as the full frame, reports `requirement`
    /// without anchor arithmetic, and panics if `resolve` is reached directly.
    #[test]
    fn anchor_absent_and_packed_resolution() {
        let geo = HashMap::new();
        let packed = Placement::packed(Size([SizeValue::fill(), SizeValue::fixed(20.0)]));

        let spec_0 = source_of(&packed, 0, &geo);
        assert_eq!(spec_0.anchor, Anchor::Absent);
        assert_eq!(available(100.0, &spec_0), 100.0);
        assert_eq!(requirement(&spec_0, 45.0), 45.0);

        let panic_res = std::panic::catch_unwind(|| spec_0.anchor.resolve(100.0));
        assert!(panic_res.is_err(), "Anchor::Absent must panic on resolve()");

        assert_eq!(fits_frame(0, 10.0, 50.0, 100.0), Ok(()));
        assert_eq!(
            fits_frame(0, 110.0, 10.0, 100.0),
            Err(Violation::AnchorBeyondFrame { axis: 0 })
        );
        assert_eq!(
            fits_frame(0, 90.0, 20.0, 100.0),
            Err(Violation::ExtentBeyondFrame { axis: 0 })
        );

        // resolve_packed checks
        let res_ok = resolve_packed(
            &Placement::packed(Size([SizeValue::fixed(30.0), SizeValue::fixed(20.0)])),
            (50.0, 50.0),
            &geo,
            [None, None],
        );
        assert_eq!(res_ok, Ok((30.0, 20.0)));

        let res_nonpositive = resolve_packed(
            &Placement::packed(Size([SizeValue::fixed(-5.0), SizeValue::fixed(20.0)])),
            (50.0, 50.0),
            &geo,
            [None, None],
        );
        assert_eq!(
            res_nonpositive,
            Err(Violation::AuthoredExtentNotPositive {
                axis: 0,
                written_as_to: false,
            })
        );

        let res_overrun = resolve_packed(
            &Placement::packed(Size([SizeValue::fixed(60.0), SizeValue::fixed(20.0)])),
            (50.0, 50.0),
            &geo,
            [None, None],
        );
        assert_eq!(res_overrun, Err(Violation::ExtentBeyondFrame { axis: 0 }));
    }

    /// Proves that `arrange_flow` accumulates extents along the primary axis, spaces occupying
    /// children with gaps without leading or trailing margins, and preserves secondary axis extents.
    #[test]
    fn flow_row_arrangement_with_gaps_and_zero_extent() {
        let flow = Flow {
            direction: FlowDirection::Row,
            gap: 5.0,
        };
        // 3 active children (caller filters inactive children before calling arrange_flow)
        let children = vec![
            FlowChildInput {
                resolved_box: (20.0, 30.0),
                requirement: (20.0, 30.0),
            },
            FlowChildInput {
                resolved_box: (0.0, 25.0),
                requirement: (0.0, 25.0),
            },
            FlowChildInput {
                resolved_box: (30.0, 40.0),
                requirement: (30.0, 40.0),
            },
        ];

        let res = arrange_flow((100.0, 50.0), &flow, &children).expect("arrange row");
        assert_eq!(res.rects.len(), 3);
        assert_eq!(
            res.rects[0],
            Placed {
                x: 0.0,
                y: 20.0,
                w: 20.0,
                h: 30.0,
            }
        );
        // Middle zero-extent child sits at the leading edge the next occupying child takes (25.0)
        assert_eq!(
            res.rects[1],
            Placed {
                x: 25.0,
                y: 25.0,
                w: 0.0,
                h: 25.0,
            }
        );
        assert_eq!(
            res.rects[2],
            Placed {
                x: 25.0,
                y: 10.0,
                w: 30.0,
                h: 40.0,
            }
        );
        assert_eq!(res.assembled, (55.0, 40.0));
    }

    /// Proves that a trailing zero-extent child is placed at `cursor` (without a trailing gap)
    /// and does not cause an overrun in a frame sized exactly to the occupying children.
    #[test]
    fn flow_trailing_zero_extent_child_places_at_cursor_without_gap() {
        let flow = Flow {
            direction: FlowDirection::Row,
            gap: 4.0,
        };
        let children = vec![
            FlowChildInput {
                resolved_box: (20.0, 10.0),
                requirement: (20.0, 10.0),
            },
            FlowChildInput {
                resolved_box: (20.0, 10.0),
                requirement: (20.0, 10.0),
            },
            FlowChildInput {
                resolved_box: (0.0, 10.0),
                requirement: (0.0, 10.0),
            },
        ];

        // Inner width 44.0 exactly matches occupying children (20 + 4 + 20 = 44)
        let res = arrange_flow((44.0, 20.0), &flow, &children).expect("trailing zero extent");
        assert_eq!(res.rects.len(), 3);
        assert_eq!(res.rects[0].x, 0.0);
        assert_eq!(res.rects[1].x, 24.0);
        // Trailing zero-extent child sits at cursor (44.0), not cursor + gap (48.0)
        assert_eq!(res.rects[2].x, 44.0);
        assert_eq!(res.assembled, (44.0, 10.0));
    }

    #[test]
    fn flow_column_arrangement_with_gaps_and_drawing_coords() {
        let flow = Flow {
            direction: FlowDirection::Column,
            gap: 10.0,
        };
        let children = vec![
            FlowChildInput {
                resolved_box: (30.0, 20.0),
                requirement: (30.0, 20.0),
            },
            FlowChildInput {
                resolved_box: (35.0, 30.0),
                requirement: (35.0, 30.0),
            },
        ];

        let res = arrange_flow((40.0, 100.0), &flow, &children).expect("arrange column");
        assert_eq!(res.rects.len(), 2);
        assert_eq!(
            res.rects[0],
            Placed {
                x: 0.0,
                y: 80.0,
                w: 30.0,
                h: 20.0,
            }
        );
        assert_eq!(
            res.rects[1],
            Placed {
                x: 0.0,
                y: 40.0,
                w: 35.0,
                h: 30.0,
            }
        );
        assert_eq!(res.assembled, (35.0, 60.0));
    }

    #[test]
    fn flow_overflow_reports_child_index_and_axis() {
        let row_flow = Flow {
            direction: FlowDirection::Row,
            gap: 5.0,
        };
        let row_children = vec![
            FlowChildInput {
                resolved_box: (20.0, 10.0),
                requirement: (20.0, 10.0),
            },
            FlowChildInput {
                resolved_box: (15.0, 10.0),
                requirement: (15.0, 10.0),
            },
        ];
        let row_err = arrange_flow((30.0, 50.0), &row_flow, &row_children).expect_err("overflow");
        assert_eq!(row_err, (1, Violation::ExtentBeyondFrame { axis: 0 }));

        let col_flow = Flow {
            direction: FlowDirection::Column,
            gap: 5.0,
        };
        let col_children = vec![
            FlowChildInput {
                resolved_box: (10.0, 20.0),
                requirement: (10.0, 20.0),
            },
            FlowChildInput {
                resolved_box: (10.0, 15.0),
                requirement: (10.0, 15.0),
            },
        ];
        let col_err = arrange_flow((50.0, 30.0), &col_flow, &col_children).expect_err("overflow");
        assert_eq!(col_err, (1, Violation::ExtentBeyondFrame { axis: 1 }));
    }
}
