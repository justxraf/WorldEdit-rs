//! Block position and state rotation/flip transformations for clipboard
//! `//rotate` and `//flip`.
//!
//! A [`Transform`] is a signed permutation matrix: 90-degree-multiple
//! rotations (any axis) and per-axis flips all compose into a single 3x3
//! integer matrix with exactly one nonzero (+1 or -1) entry per row and
//! column. Composition is matrix multiplication, so repeated
//! `//rotate`/`//flip` calls combine exactly like FAWE's
//! `AffineTransform::combine`, and is always exact (no resampling).
//!
//! Positions are transformed around the clipboard's origin point (the
//! position recorded at `//copy` time, i.e. where `-o` pastes), not the
//! bounding-box center: this keeps every transform exactly integer-valued
//! and trivially composable, at the cost of the structure's footprint
//! shifting relative to its origin if the original selection wasn't
//! centered on the copy point. `//paste` recomputes its target bounding box
//! from the transformed offsets, so the pasted structure is still placed
//! correctly - only its position *relative to the origin point* changes.

use crate::mapping;

const IDENTITY: [[i32; 3]; 3] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
/// 90° rotation around Y: (x, y, z) -> (-z, y, x). Matches WorldEdit's
/// "positive angle = clockwise" convention (north -> east).
const ROT_Y_90: [[i32; 3]; 3] = [[0, 0, -1], [0, 1, 0], [1, 0, 0]];
/// 90° rotation around X: (x, y, z) -> (x, -z, y).
const ROT_X_90: [[i32; 3]; 3] = [[1, 0, 0], [0, 0, -1], [0, 1, 0]];
/// 90° rotation around Z: (x, y, z) -> (-y, x, z).
const ROT_Z_90: [[i32; 3]; 3] = [[0, -1, 0], [1, 0, 0], [0, 0, 1]];

/// A pending rotation/flip to apply to the clipboard, represented as a 3x3
/// signed permutation matrix. The identity transform leaves positions and
/// states unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Transform {
    matrix: [[i32; 3]; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self { matrix: IDENTITY }
    }
}

impl Transform {
    /// The identity transform (no change).
    pub fn identity() -> Self {
        Self::default()
    }

    /// A rotation around the Y (vertical) axis. `degrees` must be a multiple
    /// of 90 (negative and >360 values are normalized); returns `None`
    /// otherwise.
    pub fn rotate_y(degrees: i32) -> Option<Self> {
        Self::from_base(ROT_Y_90, degrees)
    }

    /// A rotation around the X axis. `degrees` must be a multiple of 90.
    pub fn rotate_x(degrees: i32) -> Option<Self> {
        Self::from_base(ROT_X_90, degrees)
    }

    /// A rotation around the Z axis. `degrees` must be a multiple of 90.
    pub fn rotate_z(degrees: i32) -> Option<Self> {
        Self::from_base(ROT_Z_90, degrees)
    }

    fn from_base(base: [[i32; 3]; 3], degrees: i32) -> Option<Self> {
        let steps = normalize_rotation(degrees)?;
        let mut matrix = IDENTITY;
        for _ in 0..steps {
            matrix = mat_mul(base, matrix);
        }
        Some(Self { matrix })
    }

    /// Mirror across the X axis (negates X offsets).
    pub fn flip_axis_x() -> Self {
        Self {
            matrix: [[-1, 0, 0], [0, 1, 0], [0, 0, 1]],
        }
    }

    /// Mirror across the Y axis (negates Y offsets).
    pub fn flip_axis_y() -> Self {
        Self {
            matrix: [[1, 0, 0], [0, -1, 0], [0, 0, 1]],
        }
    }

    /// Mirror across the Z axis (negates Z offsets).
    pub fn flip_axis_z() -> Self {
        Self {
            matrix: [[1, 0, 0], [0, 1, 0], [0, 0, -1]],
        }
    }

    /// Compose two transforms: the result applies `self` first, then `other`
    /// to the result. This matches FAWE's `combine`, where repeated
    /// `//rotate`/`//flip` calls accumulate onto the clipboard's pending
    /// transform.
    pub fn combine(&self, other: Self) -> Self {
        Self {
            matrix: mat_mul(other.matrix, self.matrix),
        }
    }

    /// `true` if this transform leaves positions and states unchanged.
    pub fn is_identity(&self) -> bool {
        self.matrix == IDENTITY
    }

    /// Apply this transform to an offset/position vector.
    pub fn apply(&self, v: (i32, i32, i32)) -> (i32, i32, i32) {
        let m = self.matrix;
        (
            m[0][0] * v.0 + m[0][1] * v.1 + m[0][2] * v.2,
            m[1][0] * v.0 + m[1][1] * v.1 + m[1][2] * v.2,
            m[2][0] * v.0 + m[2][1] * v.1 + m[2][2] * v.2,
        )
    }

    /// Determinant of the transform matrix: `+1` for proper rotations, `-1`
    /// for transforms that include an odd number of axis flips (mirrors
    /// chirality-sensitive properties like door `hinge`).
    fn determinant(&self) -> i32 {
        let m = self.matrix;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }
}

/// `result = a * b` (matrix product, applied to a column vector as `a * (b * v)`).
fn mat_mul(a: [[i32; 3]; 3], b: [[i32; 3]; 3]) -> [[i32; 3]; 3] {
    let mut result = [[0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            result[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    result
}

/// Normalize degrees to a rotation step count (0/1/2/3 for 0/90/180/270°), or
/// `None` if `degrees` isn't a multiple of 90.
fn normalize_rotation(degrees: i32) -> Option<u8> {
    match ((degrees % 360) + 360) % 360 {
        0 => Some(0),
        90 => Some(1),
        180 => Some(2),
        270 => Some(3),
        _ => None,
    }
}

/// Rotate/flip a block state id, remapping directional properties.
///
/// Splits the state into name and properties, transforms direction-valued
/// properties (`facing`, `axis`, `rotation`, `orientation`, `hinge`, `half`,
/// rail `shape`) according to the transform, and re-resolves the state. Falls
/// back to the original state if the transformed combination doesn't exist.
pub fn transform_state(state_id: u16, transform: Transform) -> u16 {
    if transform.is_identity() || state_id == 0 {
        return state_id;
    }

    let key = mapping::palette_key_for_state_id(state_id);
    let (name, props) = split_key_local(&key);

    if props.is_empty() {
        return state_id;
    }

    let transformed_props = transform_properties(props, &transform);
    let candidate = format!("{name}[{transformed_props}]");

    mapping::state_id_for(&candidate).unwrap_or(state_id)
}

/// Split a palette key into name and property string (without brackets).
fn split_key_local(key: &str) -> (&str, &str) {
    match key.split_once('[') {
        Some((name, rest)) => {
            let rest = rest.strip_suffix(']').unwrap_or(rest);
            (name.trim(), rest)
        }
        None => (key.trim(), ""),
    }
}

/// Transform a `k=v,k=v` property string according to `transform`.
fn transform_properties(props: &str, transform: &Transform) -> String {
    let mut result: Vec<String> = Vec::new();

    for prop in props.split(',') {
        let prop = prop.trim();
        let Some((key, value)) = prop.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        let new_value = match key {
            "facing" => transform_direction(value, transform).unwrap_or_else(|| value.to_string()),
            "axis" => transform_axis(value, transform).unwrap_or_else(|| value.to_string()),
            "rotation" => {
                transform_rotation16(value, transform).unwrap_or_else(|| value.to_string())
            }
            "orientation" => transform_orientation(value, transform),
            "hinge" => transform_hinge(value, transform),
            "half" => transform_half(value, transform),
            "shape" => transform_rail_shape(value, transform),
            _ => value.to_string(),
        };

        result.push(format!("{key}={new_value}"));
    }

    result.sort();
    result.join(",")
}

/// Convert a direction name to its unit vector.
fn direction_to_vector(direction: &str) -> Option<(i32, i32, i32)> {
    match direction {
        "north" => Some((0, 0, -1)),
        "south" => Some((0, 0, 1)),
        "east" => Some((1, 0, 0)),
        "west" => Some((-1, 0, 0)),
        "up" => Some((0, 1, 0)),
        "down" => Some((0, -1, 0)),
        _ => None,
    }
}

/// Convert a unit vector back to a direction name.
fn vector_to_direction(v: (i32, i32, i32)) -> Option<&'static str> {
    match v {
        (0, 0, -1) => Some("north"),
        (0, 0, 1) => Some("south"),
        (1, 0, 0) => Some("east"),
        (-1, 0, 0) => Some("west"),
        (0, 1, 0) => Some("up"),
        (0, -1, 0) => Some("down"),
        _ => None,
    }
}

/// Transform a 6-way direction property (`facing`, halves of `orientation`,
/// `shape` endpoints). Returns `None` if `value` isn't a recognized
/// direction or the transformed vector isn't (e.g. a 4-way `facing` rotated
/// onto the vertical axis by an X/Z rotation, for a block that has no
/// up/down state - the caller falls back to the original value).
fn transform_direction(value: &str, transform: &Transform) -> Option<String> {
    let vector = direction_to_vector(value)?;
    let transformed = transform.apply(vector);
    vector_to_direction(transformed).map(str::to_string)
}

/// Transform an `axis` property (`x`, `y`, or `z`) - undirected, so only the
/// axis the vector lands on matters, not its sign.
fn transform_axis(value: &str, transform: &Transform) -> Option<String> {
    let vector = match value {
        "x" => (1, 0, 0),
        "y" => (0, 1, 0),
        "z" => (0, 0, 1),
        _ => return None,
    };
    let (x, y, _z) = transform.apply(vector);
    let axis = if x != 0 {
        "x"
    } else if y != 0 {
        "y"
    } else {
        "z"
    };
    Some(axis.to_string())
}

/// Map a horizontal cardinal unit vector's `(x, z)` to the 16-step `rotation`
/// property's value at the corresponding cardinal step (0/4/8/12).
fn cardinal_angle(v: (i32, i32, i32)) -> Option<i32> {
    match (v.0, v.2) {
        (0, 1) => Some(0),  // south
        (-1, 0) => Some(4), // west
        (0, -1) => Some(8), // north
        (1, 0) => Some(12), // east
        _ => None,
    }
}

/// Transform a 16-step `rotation` property (banners, signs, skulls), where
/// each step is 22.5°. Only meaningful when the transform doesn't rotate the
/// Y axis into the horizontal plane (i.e. pure Y rotation and/or flips);
/// otherwise the value is left unchanged.
fn transform_rotation16(value: &str, transform: &Transform) -> Option<String> {
    let r: i32 = value.parse().ok()?;
    if !(0..16).contains(&r) {
        return None;
    }

    let m = transform.matrix;
    if m[1][0] != 0 || m[1][2] != 0 {
        return None;
    }

    let south_image = transform.apply((0, 0, 1));
    let east_image = transform.apply((1, 0, 0));
    let a0 = cardinal_angle(south_image)?;
    let a12 = cardinal_angle(east_image)?;

    let additive_prediction = (12 + a0).rem_euclid(16);
    let new_r = if a12 == additive_prediction {
        (r + a0).rem_euclid(16)
    } else {
        (a0 - r).rem_euclid(16)
    };
    Some(new_r.to_string())
}

/// Transform an `orientation` property (e.g. jigsaw blocks), formatted as
/// `<facing>_<rotation>` where both halves are 6-way directions.
fn transform_orientation(value: &str, transform: &Transform) -> String {
    let Some((first, second)) = value.split_once('_') else {
        return value.to_string();
    };
    let new_first = transform_direction(first, transform).unwrap_or_else(|| first.to_string());
    let new_second = transform_direction(second, transform).unwrap_or_else(|| second.to_string());
    format!("{new_first}_{new_second}")
}

/// Transform a door's `hinge` (`left`/`right`). Chirality-sensitive: swaps
/// iff the transform includes an odd number of flips (determinant `-1`).
fn transform_hinge(value: &str, transform: &Transform) -> String {
    if transform.determinant() < 0 {
        match value {
            "left" => "right".to_string(),
            "right" => "left".to_string(),
            _ => value.to_string(),
        }
    } else {
        value.to_string()
    }
}

/// Transform a `half` property (`top`/`bottom`, used by stairs). Swaps iff
/// the transform maps "up" to "down".
fn transform_half(value: &str, transform: &Transform) -> String {
    if transform.apply((0, 1, 0)) == (0, -1, 0) {
        match value {
            "top" => "bottom".to_string(),
            "bottom" => "top".to_string(),
            _ => value.to_string(),
        }
    } else {
        value.to_string()
    }
}

/// Transform a rail `shape` (straight and ascending variants; curves are left
/// unchanged).
fn transform_rail_shape(value: &str, transform: &Transform) -> String {
    let pair = match value {
        "north_south" => ("north", "south"),
        "east_west" => ("east", "west"),
        "ascending_north" => ("north", "up"),
        "ascending_south" => ("south", "up"),
        "ascending_east" => ("east", "up"),
        "ascending_west" => ("west", "up"),
        _ => return value.to_string(),
    };

    let Some(d1) = transform_direction(pair.0, transform) else {
        return value.to_string();
    };
    let Some(d2) = transform_direction(pair.1, transform) else {
        return value.to_string();
    };

    match (d1.as_str(), d2.as_str()) {
        ("north", "south") | ("south", "north") => "north_south".to_string(),
        ("east", "west") | ("west", "east") => "east_west".to_string(),
        ("north", "up") | ("up", "north") => "ascending_north".to_string(),
        ("south", "up") | ("up", "south") => "ascending_south".to_string(),
        ("east", "up") | ("up", "east") => "ascending_east".to_string(),
        ("west", "up") | ("up", "west") => "ascending_west".to_string(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_identity() {
        assert!(Transform::identity().is_identity());
    }

    #[test]
    fn four_y_rotations_equal_identity() {
        let rot = Transform::rotate_y(90).unwrap();
        let mut acc = Transform::identity();
        for _ in 0..4 {
            acc = acc.combine(rot);
        }
        assert!(acc.is_identity());
    }

    #[test]
    fn four_x_rotations_equal_identity() {
        let rot = Transform::rotate_x(90).unwrap();
        let mut acc = Transform::identity();
        for _ in 0..4 {
            acc = acc.combine(rot);
        }
        assert!(acc.is_identity());
    }

    #[test]
    fn two_flips_equal_identity() {
        let flip = Transform::flip_axis_x();
        assert!(flip.combine(flip).is_identity());
    }

    #[test]
    fn rotate_y_90_matches_worldedit_clockwise_convention() {
        // north (0,0,-1) -> east (1,0,0)
        let rot = Transform::rotate_y(90).unwrap();
        assert_eq!(rot.apply((0, 0, -1)), (1, 0, 0));
        assert_eq!(rot.apply((1, 0, 0)), (0, 0, 1)); // east -> south
    }

    #[test]
    fn rotate_y_180_negates_horizontal_offsets() {
        let rot = Transform::rotate_y(180).unwrap();
        assert_eq!(rot.apply((3, 5, -2)), (-3, 5, 2));
    }

    #[test]
    fn rejects_non_multiple_of_90() {
        assert!(Transform::rotate_y(45).is_none());
        assert!(Transform::rotate_y(91).is_none());
    }

    #[test]
    fn negative_and_large_angles_normalize() {
        assert_eq!(Transform::rotate_y(-90), Transform::rotate_y(270));
        assert_eq!(Transform::rotate_y(450), Transform::rotate_y(90));
    }

    #[test]
    fn composing_two_90_y_rotations_equals_180() {
        let rot90 = Transform::rotate_y(90).unwrap();
        let rot180 = Transform::rotate_y(180).unwrap();
        assert_eq!(rot90.combine(rot90), rot180);
    }

    #[test]
    fn combine_applies_self_then_other() {
        // rotate 90, then flip X: a point rotated to east(1,0,0) should then be
        // mirrored to west(-1,0,0).
        let rot = Transform::rotate_y(90).unwrap();
        let flip = Transform::flip_axis_x();
        let combined = rot.combine(flip);
        assert_eq!(combined.apply((0, 0, -1)), (-1, 0, 0)); // north -> east -> west
    }

    #[test]
    fn oak_stairs_facing_north_rotates_to_east() {
        let stone_brick_stairs_north = mapping::state_id_for(
            "minecraft:oak_stairs[facing=north,half=bottom,shape=straight,waterlogged=false]",
        );
        let Some(state) = stone_brick_stairs_north else {
            // Full registry not embedded in this build; skip.
            return;
        };
        let rot = Transform::rotate_y(90).unwrap();
        let transformed = transform_state(state, rot);
        let key = mapping::palette_key_for_state_id(transformed);
        assert!(key.contains("facing=east"), "expected facing=east in {key}");
    }

    #[test]
    fn flip_x_mirrors_east_west_facing() {
        let new_value = transform_direction("east", &Transform::flip_axis_x());
        assert_eq!(new_value, Some("west".to_string()));
        let unchanged = transform_direction("north", &Transform::flip_axis_x());
        assert_eq!(unchanged, Some("north".to_string()));
    }

    #[test]
    fn axis_rotates_with_y_rotation() {
        let rot = Transform::rotate_y(90).unwrap();
        assert_eq!(transform_axis("x", &rot), Some("z".to_string()));
        assert_eq!(transform_axis("z", &rot), Some("x".to_string()));
        assert_eq!(transform_axis("y", &rot), Some("y".to_string()));
    }

    #[test]
    fn hinge_swaps_on_flip_but_not_rotation() {
        let rot = Transform::rotate_y(90).unwrap();
        let flip = Transform::flip_axis_x();
        assert_eq!(transform_hinge("left", &rot), "left");
        assert_eq!(transform_hinge("left", &flip), "right");
    }

    #[test]
    fn half_swaps_on_y_flip_but_not_y_rotation() {
        let rot = Transform::rotate_y(90).unwrap();
        let flip_y = Transform::flip_axis_y();
        assert_eq!(transform_half("top", &rot), "top");
        assert_eq!(transform_half("top", &flip_y), "bottom");
    }

    #[test]
    fn rail_shape_ascending_rotates() {
        let rot = Transform::rotate_y(90).unwrap();
        assert_eq!(
            transform_rail_shape("ascending_north", &rot),
            "ascending_east"
        );
        assert_eq!(transform_rail_shape("north_south", &rot), "east_west");
    }

    #[test]
    fn rail_shape_ascending_flips_with_axis_mirror() {
        let flip_z = Transform::flip_axis_z();
        assert_eq!(
            transform_rail_shape("ascending_north", &flip_z),
            "ascending_south"
        );
    }

    #[test]
    fn rotation16_rotates_with_y_rotation() {
        let rot = Transform::rotate_y(90).unwrap();
        assert_eq!(transform_rotation16("0", &rot), Some("4".to_string()));
        assert_eq!(transform_rotation16("12", &rot), Some("0".to_string()));
    }

    #[test]
    fn rotation16_unaffected_by_x_rotation_into_vertical() {
        let rot_x = Transform::rotate_x(90).unwrap();
        assert_eq!(transform_rotation16("0", &rot_x), None);
    }
}
