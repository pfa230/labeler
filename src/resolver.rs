use std::collections::HashMap;

use crate::models::{DynamicValue, Extent, Padding, Placement, Rotation, SizeValue};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    /// Non-negative coordinate `v` (e.g. 0.0, 10.0). Low edge is at `v`.
    Plain(f32),
    /// Sign-negative coordinate with inset `a = -v` (e.g. -0.0 has inset 0.0, -2.0 has inset 2.0).
    /// Low edge is at `frame - a`.
    EdgeRelative(f32),
}

impl Anchor {
    pub fn is_edge_relative(&self) -> bool {
        matches!(self, Anchor::EdgeRelative(_))
    }

    pub fn inset(&self) -> Option<f32> {
        match self {
            Anchor::Plain(_) => None,
            Anchor::EdgeRelative(a) => Some(*a),
        }
    }

    pub fn resolve(&self, frame: f32) -> f32 {
        match self {
            Anchor::Plain(v) => *v,
            Anchor::EdgeRelative(a) => frame - *a,
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
    let at_raw = placement.at.0[axis];
    let at_sign_neg = at_raw.is_sign_negative();
    let a = if at_sign_neg { -at_raw } else { 0.0 };
    let anchor = if at_sign_neg {
        Anchor::EdgeRelative(a)
    } else {
        Anchor::Plain(at_raw)
    };

    match &placement.extent {
        Extent::Size(size) => {
            let sv = &size.0[axis];
            let source = match sv {
                SizeValue::Content => ExtentSource::Content,
                SizeValue::Fill => ExtentSource::Frame,
                SizeValue::Dynamic(DynamicValue::Literal(v)) => ExtentSource::Author(*v),
                SizeValue::Dynamic(DynamicValue::Ref(ref_name)) => {
                    let v = geometry_values.get(ref_name).copied().unwrap_or(0.0);
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
    for (axis, anchor, extent) in [(0usize, x, w), (1usize, y, h)] {
        let limit = frame_axis(frame, axis);
        if anchor > limit + BOUNDS_EPSILON {
            return Err(Violation::AnchorBeyondFrame { axis });
        }
        if anchor + extent > limit + BOUNDS_EPSILON {
            return Err(Violation::ExtentBeyondFrame { axis });
        }
    }

    Ok(Placed {
        x,
        y,
        w: w.max(0.0),
        h: h.max(0.0),
    })
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
            at: Position(at),
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
}
