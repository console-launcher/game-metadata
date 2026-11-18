// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Float helpers (`no_std`: through `libm`, single-precision results of the double-precision functions, as the JVM's
//! `kotlin.math` on `Float` computes them), column-major 4x4 matrices like GL's, and 3x4 row-major affine bone matrices.

/// A column-major 4x4 (GL's layout).
pub type Mat4 = [f32; 16];
/// A row-major 3x4 affine transform (rotation and scale in the first three columns, translation in the fourth).
pub type Mat34 = [f32; 12];

/// The 3x4 identity.
pub const IDENTITY34: Mat34 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// `sin` of `x` in double precision, rounded to single.
pub fn sin(x: f32) -> f32 {
    libm::sin(x as f64) as f32
}
/// `cos` of `x` in double precision, rounded to single.
pub fn cos(x: f32) -> f32 {
    libm::cos(x as f64) as f32
}
/// `tan` of `x` in double precision, rounded to single.
pub fn tan(x: f32) -> f32 {
    libm::tan(x as f64) as f32
}
/// `atan` of `x` in double precision, rounded to single.
pub fn atan(x: f32) -> f32 {
    libm::atan(x as f64) as f32
}
/// The correctly rounded square root.
pub fn sqrt(x: f32) -> f32 {
    libm::sqrtf(x)
}
/// Floor.
pub fn floor(x: f32) -> f32 {
    libm::floorf(x)
}
/// Ceiling.
pub fn ceil(x: f32) -> f32 {
    libm::ceilf(x)
}
/// Degrees to radians in double precision, as the JVM's `Math.toRadians` computes it.
pub fn to_radians(degrees: f64) -> f64 {
    degrees * 0.017453292519943295
}
/// `tan` in double precision.
pub fn tan64(x: f64) -> f64 {
    libm::tan(x)
}
/// Kotlin's `coerceIn`: NaN stays NaN.
pub fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

/// The 4x4 identity.
pub fn identity() -> Mat4 {
    let mut o = [0.0; 16];
    o[0] = 1.0;
    o[5] = 1.0;
    o[10] = 1.0;
    o[15] = 1.0;
    o
}

/// `a * b`.
pub fn multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            let mut s = 0.0f32;
            for k in 0..4 {
                s += a[k * 4 + r] * b[c * 4 + k];
            }
            o[c * 4 + r] = s;
        }
    }
    o
}

/// An orthographic projection `width` x `height` units.
pub fn orthographic(width: f32, height: f32, near: f32, far: f32) -> Mat4 {
    let mut o = identity();
    o[0] = 2.0 / width;
    o[5] = 2.0 / height;
    o[10] = -2.0 / (far - near);
    o[14] = -(far + near) / (far - near);
    o
}

/// A perspective projection.
pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / tan(fov_y / 2.0);
    let mut o = [0.0; 16];
    o[0] = f / aspect;
    o[5] = f;
    o[10] = (far + near) / (near - far);
    o[11] = -1.0;
    o[14] = 2.0 * far * near / (near - far);
    o
}

/// A look-at view matrix.
pub fn look_at(eye: &[f32; 3], target: &[f32; 3], up: &[f32; 3]) -> Mat4 {
    let mut fx = target[0] - eye[0];
    let mut fy = target[1] - eye[1];
    let mut fz = target[2] - eye[2];
    let fl = sqrt(fx * fx + fy * fy + fz * fz);
    fx /= fl;
    fy /= fl;
    fz /= fl;
    let mut sx = fy * up[2] - fz * up[1];
    let mut sy = fz * up[0] - fx * up[2];
    let mut sz = fx * up[1] - fy * up[0];
    let sl = sqrt(sx * sx + sy * sy + sz * sz);
    sx /= sl;
    sy /= sl;
    sz /= sl;
    let ux = sy * fz - sz * fy;
    let uy = sz * fx - sx * fz;
    let uz = sx * fy - sy * fx;
    let mut o = identity();
    o[0] = sx;
    o[4] = sy;
    o[8] = sz;
    o[1] = ux;
    o[5] = uy;
    o[9] = uz;
    o[2] = -fx;
    o[6] = -fy;
    o[10] = -fz;
    o[12] = -(sx * eye[0] + sy * eye[1] + sz * eye[2]);
    o[13] = -(ux * eye[0] + uy * eye[1] + uz * eye[2]);
    o[14] = fx * eye[0] + fy * eye[1] + fz * eye[2];
    o
}

/// A row-major 3x4 applied to a point.
pub fn transform_point(m: &Mat34, x: f32, y: f32, z: f32) -> [f32; 3] {
    [
        m[0] * x + m[1] * y + m[2] * z + m[3],
        m[4] * x + m[5] * y + m[6] * z + m[7],
        m[8] * x + m[9] * y + m[10] * z + m[11],
    ]
}

/// A row-major 3x4 as a column-major 4x4.
pub fn from_rows_3x4(m: &Mat34) -> Mat4 {
    let mut o = identity();
    for r in 0..3 {
        for c in 0..4 {
            o[c * 4 + r] = m[r * 4 + c];
        }
    }
    o
}

/// `a * b` for 3x4 affine matrices.
pub fn multiply34(a: &Mat34, b: &Mat34) -> Mat34 {
    let mut o = [0.0; 12];
    for r in 0..3 {
        for c in 0..4 {
            o[r * 4 + c] = a[r * 4] * b[c]
                + a[r * 4 + 1] * b[4 + c]
                + a[r * 4 + 2] * b[8 + c]
                + if c == 3 { a[r * 4 + 3] } else { 0.0 };
        }
    }
    o
}

/// The inverse of an affine 3x4; the identity for one that cannot be inverted (a zero scale).
pub fn invert34(m: &Mat34) -> Mat34 {
    let (a, b, c, d, e, f, g, h, i) = (m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]);
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-12 {
        return IDENTITY34;
    }
    let inv = 1.0 / det;
    let r = [
        (e * i - f * h) * inv,
        (c * h - b * i) * inv,
        (b * f - c * e) * inv,
        (f * g - d * i) * inv,
        (a * i - c * g) * inv,
        (c * d - a * f) * inv,
        (d * h - e * g) * inv,
        (b * g - a * h) * inv,
        (a * e - b * d) * inv,
    ];
    let (tx, ty, tz) = (m[3], m[7], m[11]);
    [
        r[0],
        r[1],
        r[2],
        -(r[0] * tx + r[1] * ty + r[2] * tz),
        r[3],
        r[4],
        r[5],
        -(r[3] * tx + r[4] * ty + r[5] * tz),
        r[6],
        r[7],
        r[8],
        -(r[6] * tx + r[7] * ty + r[8] * tz),
    ]
}

/// T * Rz * Ry * Rx * S, with `euler` in radians.
pub fn local(scale: &[f32; 3], euler: &[f32; 3], translation: &[f32; 3]) -> Mat34 {
    let (cx, sx, cy, sy, cz, sz) =
        (cos(euler[0]), sin(euler[0]), cos(euler[1]), sin(euler[1]), cos(euler[2]), sin(euler[2]));
    let r = [
        cz * cy,
        cz * sy * sx - sz * cx,
        cz * sy * cx + sz * sx,
        sz * cy,
        sz * sy * sx + cz * cx,
        sz * sy * cx - cz * sx,
        -sy,
        cy * sx,
        cy * cx,
    ];
    compose(&r, scale, translation)
}

/// T * R(quaternion, normalised) * S.
pub fn local_quat(scale: &[f32; 3], q: &[f32; 4], translation: &[f32; 3]) -> Mat34 {
    let n0 = sqrt(q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]);
    let n = if n0 > 1e-6 { n0 } else { 1.0 };
    let (x, y, z, w) = (q[0] / n, q[1] / n, q[2] / n, q[3] / n);
    let r = [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y - z * w),
        2.0 * (x * z + y * w),
        2.0 * (x * y + z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z - x * w),
        2.0 * (x * z - y * w),
        2.0 * (y * z + x * w),
        1.0 - 2.0 * (x * x + y * y),
    ];
    compose(&r, scale, translation)
}

fn compose(r: &[f32; 9], s: &[f32; 3], t: &[f32; 3]) -> Mat34 {
    [
        r[0] * s[0],
        r[1] * s[1],
        r[2] * s[2],
        t[0],
        r[3] * s[0],
        r[4] * s[1],
        r[5] * s[2],
        t[1],
        r[6] * s[0],
        r[7] * s[1],
        r[8] * s[2],
        t[2],
    ]
}

/// The length of a 3-vector.
pub fn length(a: f32, b: f32, c: f32) -> f32 {
    sqrt(a * a + b * b + c * c)
}

/// `a x b`.
pub fn cross(a: &[f32; 3], b: &[f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// `a . b`.
pub fn dot(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `v` normalised, zero for a zero vector.
pub fn normalized(v: &[f32; 3]) -> [f32; 3] {
    let l = length(v[0], v[1], v[2]);
    if l > 0.0 { [v[0] / l, v[1] / l, v[2] / l] } else { [0.0; 3] }
}

/// Normalises `a` in place (unchanged when zero).
pub fn normalize(a: &mut [f32; 3]) {
    let l = sqrt(a[0] * a[0] + a[1] * a[1] + a[2] * a[2]);
    if l > 0.0 {
        a[0] /= l;
        a[1] /= l;
        a[2] /= l;
    }
}
