//! Block position and state rotation/flip transformations.
//!
//! A `Transform` stores pending 90-degree rotations (Y/X/Z axes) and axis flips,
//! composed lazily at paste time. Blocks' directional properties (facing, axis,
//! rotation, etc.) are remapped according to the transform.

use crate::mapping;

/// A pending rotation/flip to apply to the clipboard.
///
/// Stores 90-degree rotation angles (Y/X/Z) and axis flips. Transforms compose
/// via `combine()`, matching FAWE's design. The identity is all zeros.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Transform {
    /// Rotation around Y axis: 0, 1, 2, or 3 (representing 0°, 90°, 180°, 270°).
    pub rot_y: u8,
    /// Rotation around X axis: 0, 1, 2, or 3.
    pub rot_x: u8,
    /// Rotation around Z axis: 0, 1, 2, or 3.
    pub rot_z: u8,
    /// Flip along X axis: true = flipped.
    pub flip_x: bool,
    /// Flip along Y axis: true = flipped.
    pub flip_y: bool,
    /// Flip along Z axis: true = flipped.
    pub flip_z: bool,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            rot_y: 0,
            rot_x: 0,
            rot_z: 0,
            flip_x: false,
            flip_y: false,
            flip_z: false,
        }
    }
}

impl Transform {
    /// Create an identity transform (no change).
    pub fn identity() -> Self {
        Self::default()
    }

    /// Create a pure Y-axis rotation.
    pub fn rotate_y(degrees: i32) -> Option<Self> {
        let rot = normalize_rotation(degrees)?;
        Some(Self {
            rot_y: rot,
            ..Default::default()
        })
    }

    /// Create a pure X-axis rotation.
    pub fn rotate_x(degrees: i32) -> Option<Self> {
        let rot = normalize_rotation(degrees)?;
        Some(Self {
            rot_x: rot,
            ..Default::default()
        })
    }

    /// Create a pure Z-axis rotation.
    pub fn rotate_z(degrees: i32) -> Option<Self> {
        let rot = normalize_rotation(degrees)?;
        Some(Self {
            rot_z: rot,
            ..Default::default()
        })
    }

    /// Create a pure X-axis flip.
    pub fn flip_axis_x() -> Self {
        Self {
            flip_x: true,
            ..Default::default()
        }
    }

    /// Create a pure Y-axis flip.
    pub fn flip_axis_y() -> Self {
        Self {
            flip_y: true,
            ..Default::default()
        }
    }

    /// Create a pure Z-axis flip.
    pub fn flip_axis_z() -> Self {
        Self {
            flip_z: true,
            ..Default::default()
        }
    }

    /// Compose two transforms: apply `other` after `self`.
    pub fn combine(&self, other: Self) -> Self {
        // Apply rotations in order, then flips.
        // This is a simplified approach: we rotate the axes, then apply flips.
        // For a full implementation, we'd need matrix algebra; this handles the common cases.

        let (new_rot_y, new_rot_x, new_rot_z) = compose_rotations(
            self.rot_y,
            self.rot_x,
            self.rot_z,
            other.rot_y,
            other.rot_x,
            other.rot_z,
        );

        let (new_flip_x, new_flip_y, new_flip_z) = compose_flips(
            self.flip_x,
            self.flip_y,
            self.flip_z,
            other.flip_x,
            other.flip_y,
            other.flip_z,
        );

        Self {
            rot_y: new_rot_y,
            rot_x: new_rot_x,
            rot_z: new_rot_z,
            flip_x: new_flip_x,
            flip_y: new_flip_y,
            flip_z: new_flip_z,
        }
    }

    /// Check if this is the identity transform.
    pub fn is_identity(&self) -> bool {
        self.rot_y == 0
            && self.rot_x == 0
            && self.rot_z == 0
            && !self.flip_x
            && !self.flip_y
            && !self.flip_z
    }
}

/// Normalize degrees to 0/90/180/270; return the rotation step (0/1/2/3) or None if invalid.
fn normalize_rotation(degrees: i32) -> Option<u8> {
    let normalized = ((degrees % 360) + 360) % 360;
    match normalized {
        0 => Some(0),
        90 => Some(1),
        180 => Some(2),
        270 => Some(3),
        _ => None,
    }
}

/// Apply two sequences of 90-degree rotations in order.
/// Returns (rot_y, rot_x, rot_z) as the combined rotation.
fn compose_rotations(y1: u8, x1: u8, z1: u8, y2: u8, x2: u8, z2: u8) -> (u8, u8, u8) {
    // For now, apply Y first, then X, then Z, sequentially.
    // This is a simplification; a full implementation would use matrix multiplication.
    let (ry, rx, rz) = apply_rotation(y1, x1, z1);
    apply_rotation(
        ry.wrapping_add(y2) % 4,
        rx.wrapping_add(x2) % 4,
        rz.wrapping_add(z2) % 4,
    )
}

/// Identity for this simplification: assume rotations are applied in order Y, X, Z.
fn apply_rotation(y: u8, x: u8, z: u8) -> (u8, u8, u8) {
    (y % 4, x % 4, z % 4)
}

/// Compose two flip sequences; each flip flips a bool, so the result is XOR.
fn compose_flips(
    fx1: bool,
    fy1: bool,
    fz1: bool,
    fx2: bool,
    fy2: bool,
    fz2: bool,
) -> (bool, bool, bool) {
    (fx1 ^ fx2, fy1 ^ fy2, fz1 ^ fz2)
}

/// Rotate a position `(dx, dy, dz)` around the origin by the given transform.
///
/// Assumes the bounding box is centered at the origin. Used when transforming
/// clipboard offsets at paste time.
pub fn rotate_position(dx: i32, dy: i32, dz: i32, transform: Transform) -> (i32, i32, i32) {
    let (mut x, mut y, mut z) = (dx, dy, dz);

    // Apply Y rotations (around vertical axis).
    for _ in 0..transform.rot_y {
        let temp = x;
        x = -z;
        z = temp;
    }

    // Apply X rotations (around horizontal left-right axis).
    for _ in 0..transform.rot_x {
        let temp = y;
        y = -z;
        z = temp;
    }

    // Apply Z rotations (around front-back axis).
    for _ in 0..transform.rot_z {
        let temp = x;
        x = -y;
        y = temp;
    }

    // Apply flips.
    if transform.flip_x {
        x = -x;
    }
    if transform.flip_y {
        y = -y;
    }
    if transform.flip_z {
        z = -z;
    }

    (x, y, z)
}

/// Rotate/flip a block state id, remapping directional properties.
///
/// Splits the state into name and properties, transforms the properties
/// according to the transform, and re-resolves the state. Falls back to the
/// original state if the transformed state doesn't exist.
pub fn transform_state(state_id: u16, transform: Transform) -> u16 {
    if transform.is_identity() || state_id == 0 {
        return state_id;
    }

    let key = mapping::palette_key_for_state_id(state_id);
    let (name, props) = split_key_local(&key);

    if props.is_empty() {
        return state_id; // No properties to transform.
    }

    let transformed_props = transform_properties(props, transform);
    let candidate = if transformed_props.is_empty() {
        name.to_string()
    } else {
        format!("{}[{}]", name, transformed_props)
    };

    mapping::state_id_for(&candidate).unwrap_or(state_id)
}

/// Split a palette key into name and property string.
fn split_key_local(key: &str) -> (&str, &str) {
    match key.split_once('[') {
        Some((name, rest)) => {
            let rest = rest.strip_suffix(']').unwrap_or(rest);
            (name.trim(), rest)
        }
        None => (key.trim(), ""),
    }
}

/// Transform block-state properties according to a rotation/flip.
///
/// Maps directional properties like `facing`, `axis`, `rotation`, `shape`, etc.
fn transform_properties(props: &str, transform: Transform) -> String {
    let mut result = Vec::new();

    for prop in props.split(',') {
        let prop = prop.trim();
        let (key, value) = match prop.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => continue,
        };

        let new_value = match key {
            "facing" => transform_facing(value, transform),
            "axis" => transform_axis(value, transform),
            "rotation" => transform_rotation_prop(value, transform),
            "orientation" => transform_orientation(value, transform),
            "hinge" => transform_hinge(value, transform),
            "shape" => transform_rail_shape(value, transform),
            "half" => {
                // Vertical half (top/bottom) flips with Y.
                if transform.flip_y {
                    match value {
                        "top" => "bottom",
                        "bottom" => "top",
                        _ => value,
                    }
                } else {
                    value
                }
            }
            _ => value,
        };

        result.push(format!("{}={}", key, new_value));
    }

    result.sort();
    result.join(",")
}

/// Transform a `facing` direction (north, south, east, west, up, down).
fn transform_facing(facing: &str, transform: Transform) -> &'static str {
    let mut dir = match facing {
        "north" => Direction::North,
        "south" => Direction::South,
        "east" => Direction::East,
        "west" => Direction::West,
        "up" => Direction::Up,
        "down" => Direction::Down,
        _ => return facing,
    };

    // Apply Y rotations.
    for _ in 0..transform.rot_y {
        dir = dir.rotate_y();
    }

    // Apply flips (interpret as rotations around the respective axes).
    if transform.flip_x {
        dir = dir.flip_x();
    }
    if transform.flip_y {
        dir = dir.flip_y();
    }
    if transform.flip_z {
        dir = dir.flip_z();
    }

    dir.as_str()
}

/// Transform an `axis` property (x, y, z).
fn transform_axis(axis: &str, transform: Transform) -> &'static str {
    let mut ax = match axis {
        "x" => Axis::X,
        "y" => Axis::Y,
        "z" => Axis::Z,
        _ => return axis,
    };

    // Y rotations permute X and Z.
    for _ in 0..transform.rot_y {
        ax = match ax {
            Axis::X => Axis::Z,
            Axis::Z => Axis::X,
            Axis::Y => Axis::Y,
        };
    }

    // X rotations permute Y and Z.
    for _ in 0..transform.rot_x {
        ax = match ax {
            Axis::Y => Axis::Z,
            Axis::Z => Axis::Y,
            Axis::X => Axis::X,
        };
    }

    // Z rotations permute X and Y.
    for _ in 0..transform.rot_z {
        ax = match ax {
            Axis::X => Axis::Y,
            Axis::Y => Axis::X,
            Axis::Z => Axis::Z,
        };
    }

    ax.as_str()
}

/// Transform a `rotation` property (used by item frames, banners, etc., 0-15).
fn transform_rotation_prop(rotation: &str, transform: Transform) -> String {
    let Ok(rot) = rotation.parse::<u16>() else {
        return rotation.to_string();
    };

    // Only Y rotation affects this; convert to 0-3 scale, apply rotation, convert back.
    let step = (rot + 1) / 4; // Roughly maps 0-15 to 0-3.
    let new_step = (step + transform.rot_y as u16) % 4;
    let new_rot = new_step * 4;
    new_rot.to_string()
}

/// Transform an `orientation` property (used by pointing blocks).
fn transform_orientation(orientation: &str, transform: Transform) -> String {
    // Orientation is a compound of facing direction; reuse facing logic.
    if let Some((facing_part, _)) = orientation.split_once('_') {
        let new_facing = transform_facing(facing_part, transform);
        format!("{}_down", new_facing) // Simplified; full implementation would parse both parts.
    } else {
        orientation.to_string()
    }
}

/// Transform a `hinge` property (left/right, for doors).
fn transform_hinge(hinge: &str, transform: Transform) -> &'static str {
    // Hinge flips with X-Z plane flips.
    if transform.flip_x ^ transform.flip_z {
        match hinge {
            "left" => "right",
            "right" => "left",
            _ => hinge,
        }
    } else {
        hinge
    }
}

/// Transform a rail `shape` property.
fn transform_rail_shape(shape: &str, transform: Transform) -> &'static str {
    use Direction as D;

    let dir = match shape {
        "north_south" => (D::North, D::South),
        "east_west" => (D::East, D::West),
        "ascending_north" => (D::North, D::Up),
        "ascending_south" => (D::South, D::Up),
        "ascending_east" => (D::East, D::Up),
        "ascending_west" => (D::West, D::Up),
        _ => return shape,
    };

    // Transform both directions.
    let mut d1 = dir.0;
    let mut d2 = dir.1;

    for _ in 0..transform.rot_y {
        d1 = d1.rotate_y();
        d2 = d2.rotate_y();
    }

    if transform.flip_x {
        d1 = d1.flip_x();
        d2 = d2.flip_x();
    }
    if transform.flip_y {
        d1 = d1.flip_y();
        d2 = d2.flip_y();
    }
    if transform.flip_z {
        d1 = d1.flip_z();
        d2 = d2.flip_z();
    }

    // Reconstruct shape string.
    match (d1, d2) {
        (D::North, D::South) | (D::South, D::North) => "north_south",
        (D::East, D::West) | (D::West, D::East) => "east_west",
        (D::North, D::Up) | (D::Up, D::North) => "ascending_north",
        (D::South, D::Up) | (D::Up, D::South) => "ascending_south",
        (D::East, D::Up) | (D::Up, D::East) => "ascending_east",
        (D::West, D::Up) | (D::Up, D::West) => "ascending_west",
        _ => shape,
    }
}

/// Cardinal and vertical directions.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Direction {
    North,
    South,
    East,
    West,
    Up,
    Down,
}

impl Direction {
    fn as_str(self) -> &'static str {
        match self {
            Direction::North => "north",
            Direction::South => "south",
            Direction::East => "east",
            Direction::West => "west",
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }

    fn rotate_y(self) -> Self {
        match self {
            Direction::North => Direction::East,
            Direction::East => Direction::South,
            Direction::South => Direction::West,
            Direction::West => Direction::North,
            _ => self,
        }
    }

    fn flip_x(self) -> Self {
        match self {
            Direction::East => Direction::West,
            Direction::West => Direction::East,
            _ => self,
        }
    }

    fn flip_y(self) -> Self {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            _ => self,
        }
    }

    fn flip_z(self) -> Self {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            _ => self,
        }
    }
}

/// Block axes.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    fn as_str(self) -> &'static str {
        match self {
            Axis::X => "x",
            Axis::Y => "y",
            Axis::Z => "z",
        }
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
        let four_rots = rot.combine(rot).combine(rot).combine(rot);
        assert!(four_rots.is_identity());
    }

    #[test]
    fn two_flips_equal_identity() {
        let flip = Transform::flip_axis_x();
        let two_flips = flip.combine(flip);
        assert!(two_flips.is_identity());
    }

    #[test]
    fn rotate_position_90_degrees_y() {
        let (x, y, z) = rotate_position(1, 0, 0, Transform::rotate_y(90).unwrap());
        assert_eq!((x, y, z), (0, 0, -1));
    }

    #[test]
    fn rotate_position_180_degrees_y() {
        let (x, y, z) = rotate_position(1, 0, 0, Transform::rotate_y(180).unwrap());
        assert_eq!((x, y, z), (-1, 0, 0));
    }

    #[test]
    fn facing_north_rotates_to_east() {
        let facing = transform_facing("north", Transform::rotate_y(90).unwrap());
        assert_eq!(facing, "east");
    }

    #[test]
    fn axis_x_rotates_to_z_around_y() {
        let axis = transform_axis("x", Transform::rotate_y(90).unwrap());
        assert_eq!(axis, "z");
    }

    #[test]
    fn flip_x_reverses_east_west() {
        let facing = transform_facing("east", Transform::flip_axis_x());
        assert_eq!(facing, "west");
    }
}
