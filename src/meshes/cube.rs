use crate::Vec3;

#[rustfmt::skip]
pub const POSITIONS: &[Vec3] = &[
    Vec3::new( 1.0,  1.0,  1.0),
    Vec3::new( 1.0,  1.0, -1.0),
    Vec3::new( 1.0, -1.0,  1.0),
    Vec3::new( 1.0, -1.0, -1.0),
    Vec3::new(-1.0,  1.0,  1.0),
    Vec3::new(-1.0,  1.0, -1.0),
    Vec3::new(-1.0, -1.0,  1.0),
    Vec3::new(-1.0, -1.0, -1.0),
];

#[rustfmt::skip]
pub const INDICES: &[u32] = &[
    4, 2, 0,
    2, 7, 3,
    6, 5, 7,
    1, 7, 5,
    0, 3, 1,
    4, 1, 5,
    4, 6, 2,
    2, 6, 7,
    6, 4, 5,
    1, 3, 7,
    0, 2, 3,
    4, 0, 1
];
