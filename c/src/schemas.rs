use std::ffi::c_char;

/// A vector in 3D space that represents a direction only
/// cbindgen:include
#[repr(C)]
pub struct Vector3 {
    /// x coordinate length
    pub x: f64,
    /// y coordinate length
    pub y: f64,
    /// z coordinate length
    pub z: f64,
}

impl From<&Vector3> for foxglove::schemas::Vector3 {
    fn from(msg: &Vector3) -> Self {
        Self {
            x: msg.x,
            y: msg.y,
            z: msg.z,
        }
    }
}

/// A [quaternion](<https://eater.net/quaternions>) representing a rotation in 3D space
#[repr(C)]
pub struct Quaternion {
    /// x value
    pub x: f64,
    /// y value
    pub y: f64,
    /// z value
    pub z: f64,
    /// w value
    pub w: f64,
}

/// A position and orientation for an object or reference frame in 3D space
#[repr(C)]
pub struct Pose {
    /// Point denoting position in 3D space
    pub position: *const Vector3,
    /// Quaternion denoting orientation in 3D space
    pub orientation: *const Quaternion,
}

/// A timestamp, represented as an offset from a user-defined epoch.
#[repr(C)]
pub struct Timestamp {
    /// Seconds since epoch.
    pub sec: u32,
    /// Additional nanoseconds since epoch.
    pub nsec: u32,
}

/// A timestamped pose for an object or reference frame in 3D space
#[repr(C)]
pub struct PoseInFrame {
    /// Timestamp of pose
    pub timestamp: *const Timestamp,
    /// Frame of reference for pose position and orientation
    pub frame_id: *const c_char,
    pub frame_id_len: usize,
    /// Pose in 3D space
    pub pose: *const Pose,
}

/// An array of timestamped poses for an object or reference frame in 3D space
#[repr(C)]
pub struct PosesInFrame {
    /// Timestamp of pose
    pub timestamp: *const Timestamp,
    /// Frame of reference for pose position and orientation
    pub frame_id: *const c_char,
    pub frame_id_len: usize,
    /// Poses in 3D space
    pub poses: *const Pose,
    pub poses_count: usize,
}
