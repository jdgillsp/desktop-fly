//! Column-major 4x4 transforms, matching SceneKit's conventions so the ported
//! geometry numbers from `FlyModel.swift` can be used verbatim.
//!
//! SceneKit applies `eulerAngles` as pitch (x), yaw (y), roll (z) — extrinsic
//! ZYX order, i.e. the matrix product `Rz * Ry * Rx`. Getting that order wrong
//! puts the legs on backwards, so it is pinned by a test.

pub type Mat4 = [[f32; 4]; 4];

pub fn identity() -> Mat4 {
    let mut m = [[0.0; 4]; 4];
    for i in 0..4 {
        m[i][i] = 1.0;
    }
    m
}

pub fn mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut o = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k][r] * b[c][k];
            }
            o[c][r] = s;
        }
    }
    o
}

pub fn translate(x: f32, y: f32, z: f32) -> Mat4 {
    let mut m = identity();
    m[3] = [x, y, z, 1.0];
    m
}

pub fn scale(x: f32, y: f32, z: f32) -> Mat4 {
    let mut m = identity();
    m[0][0] = x;
    m[1][1] = y;
    m[2][2] = z;
    m
}

pub fn rotate_x(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    m[1][1] = c;
    m[1][2] = s;
    m[2][1] = -s;
    m[2][2] = c;
    m
}

pub fn rotate_y(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    m[0][0] = c;
    m[0][2] = -s;
    m[2][0] = s;
    m[2][2] = c;
    m
}

pub fn rotate_z(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    m[0][0] = c;
    m[0][1] = s;
    m[1][0] = -s;
    m[1][1] = c;
    m
}

/// SceneKit `eulerAngles` -> matrix. Order is Rz * Ry * Rx.
pub fn euler(x: f32, y: f32, z: f32) -> Mat4 {
    mul(rotate_z(z), mul(rotate_y(y), rotate_x(x)))
}

/// A SceneKit node's local transform: scale, then rotate, then translate.
pub fn trs(pos: [f32; 3], eul: [f32; 3], scl: [f32; 3]) -> Mat4 {
    mul(
        translate(pos[0], pos[1], pos[2]),
        mul(euler(eul[0], eul[1], eul[2]), scale(scl[0], scl[1], scl[2])),
    )
}

pub fn transform_point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0],
        m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1],
        m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2],
    ]
}

/// Directions ignore translation. This is *not* the inverse-transpose, so it is
/// only correct for uniform scale; the fly's parts use non-uniform scale, and
/// the resulting slight normal skew is what SceneKit's default shading shows
/// too — matching it keeps the look faithful.
pub fn transform_dir(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    let v = [
        m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2],
        m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2],
        m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2],
    ];
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}

/// Orthographic projection matching the SceneKit camera in `main.swift:buildScene`
/// (`usesOrthographicProjection`, `orthographicScale = height/2`, zNear 1, zFar 600).
pub fn ortho(half_w: f32, half_h: f32, near: f32, far: f32) -> Mat4 {
    let mut m = identity();
    m[0][0] = 1.0 / half_w;
    m[1][1] = 1.0 / half_h;
    m[2][2] = 1.0 / (near - far);
    m[3][2] = near / (near - far);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4)
    }

    #[test]
    fn translate_moves_a_point() {
        let m = translate(1.0, 2.0, 3.0);
        assert!(close(transform_point(&m, [0.0, 0.0, 0.0]), [1.0, 2.0, 3.0]));
    }

    #[test]
    fn rotate_z_is_counter_clockwise_about_z() {
        let m = rotate_z(std::f32::consts::FRAC_PI_2);
        assert!(close(transform_point(&m, [1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]));
    }

    #[test]
    fn rotate_y_takes_x_toward_negative_z() {
        let m = rotate_y(std::f32::consts::FRAC_PI_2);
        assert!(close(transform_point(&m, [1.0, 0.0, 0.0]), [0.0, 0.0, -1.0]));
    }

    #[test]
    fn rotate_x_takes_y_toward_positive_z() {
        let m = rotate_x(std::f32::consts::FRAC_PI_2);
        assert!(close(transform_point(&m, [0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]));
    }

    /// Euler order is load-bearing: the legs attach through a yaw+roll chain,
    /// and applying these in the wrong order splays them the wrong way.
    #[test]
    fn euler_applies_z_then_y_then_x() {
        let (x, y, z) = (0.3f32, 0.5f32, 0.7f32);
        let expect = mul(rotate_z(z), mul(rotate_y(y), rotate_x(x)));
        let got = euler(x, y, z);
        for c in 0..4 {
            for r in 0..4 {
                assert!((expect[c][r] - got[c][r]).abs() < 1e-6);
            }
        }
    }

    /// A child's world transform is parent * child, and composing must match
    /// applying the two steps in sequence.
    #[test]
    fn hierarchy_composes() {
        let parent = trs([10.0, 0.0, 0.0], [0.0, 0.0, std::f32::consts::FRAC_PI_2], [1.0; 3]);
        let child = trs([5.0, 0.0, 0.0], [0.0; 3], [1.0; 3]);
        let world = mul(parent, child);
        // The child sits 5 along the parent's local +x, which the parent's 90 deg
        // roll has turned into world +y.
        assert!(close(transform_point(&world, [0.0, 0.0, 0.0]), [10.0, 5.0, 0.0]));
    }

    #[test]
    fn scale_then_rotate_then_translate_order() {
        // A unit x-vector scaled by 2, rotated 90 deg about z, then moved to (1,1,0)
        // must land at (1, 3, 0) — not (3, 1, 0), which is what the wrong order gives.
        let m = trs(
            [1.0, 1.0, 0.0],
            [0.0, 0.0, std::f32::consts::FRAC_PI_2],
            [2.0, 2.0, 2.0],
        );
        assert!(close(transform_point(&m, [1.0, 0.0, 0.0]), [1.0, 3.0, 0.0]));
    }

    #[test]
    fn ortho_maps_the_view_box_to_clip_space() {
        let m = ortho(100.0, 50.0, 1.0, 600.0);
        let p = transform_point(&m, [100.0, 50.0, 0.0]);
        assert!((p[0] - 1.0).abs() < 1e-5 && (p[1] - 1.0).abs() < 1e-5);
    }
}
