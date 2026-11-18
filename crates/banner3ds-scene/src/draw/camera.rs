// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cameras and framing: HOME's fixed view for every banner, a file camera's view, the letterbox viewport, and HOME's window
//! (the ellipse the HOME Menu shows every banner through).

use super::math::{self, Mat4};
use super::pose::Player;
use super::skin;
use crate::{BannerFileCamera, BannerScene, CameraView};

/// A look-at camera; matrices are column-major 4x4 like GL's.
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    /// Where it stands.
    pub eye: [f32; 3],
    /// What it looks at.
    pub target: [f32; 3],
    /// Up.
    pub up: [f32; 3],
    /// Vertical field of view, radians.
    pub fov_y: f32,
    /// Near plane.
    pub near: f32,
    /// Far plane.
    pub far: f32,
    /// The picture's own width over height when the camera fixes it (the renderers letterbox to it), `None` to fill the output.
    pub aspect: Option<f32>,
    /// Above zero: orthographic, this many units tall.
    pub orthographic_height: f32,
    /// The picture shows through HOME's window ([`home_window`]), as HOME's own view does.
    pub home_window: bool,
}

impl Camera {
    /// The view matrix.
    pub fn view(&self) -> Mat4 {
        math::look_at(&self.eye, &self.target, &self.up)
    }

    /// The projection at `aspect` (width over height).
    pub fn projection(&self, aspect: f32) -> Mat4 {
        if self.orthographic_height > 0.0 {
            math::orthographic(self.orthographic_height * aspect, self.orthographic_height, self.near, self.far)
        } else {
            math::perspective(self.fov_y, aspect, self.near, self.far)
        }
    }

    /// A file camera's view: where it stands, how it aims, its projection (aspect 5:3 when the file's is unusable).
    pub fn of_file(file: &BannerFileCamera) -> Camera {
        let eye = file.position;
        let (target, up) = match &file.view {
            CameraView::LookAt { target, up } => (*target, *up),
            CameraView::Aim { target, twist } => {
                (*target, twisted(&[target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]], *twist))
            }
            CameraView::Rotate { rotation } => {
                let m = math::local(&[1.0; 3], rotation, &eye);
                ([eye[0] - m[2], eye[1] - m[6], eye[2] - m[10]], [m[1], m[5], m[9]])
            }
        };
        let p = &file.projection;
        Camera {
            eye,
            target,
            up,
            fov_y: if p.orthographic { 0.0 } else { p.fov_or_height },
            near: p.near,
            far: p.far,
            aspect: Some(if p.aspect > 0.0 && p.aspect.is_finite() { p.aspect } else { 5.0 / 3.0 }),
            orthographic_height: if p.orthographic { p.fov_or_height } else { 0.0 },
            home_window: false,
        }
    }
}

/// +Y rolled `twist` radians about `forward` (Rodrigues).
fn twisted(forward: &[f32; 3], twist: f32) -> [f32; 3] {
    if twist == 0.0 {
        return [0.0, 1.0, 0.0];
    }
    let l = math::length(forward[0], forward[1], forward[2]);
    if l <= 1e-6 {
        return [0.0, 1.0, 0.0];
    }
    let (kx, ky, kz) = (forward[0] / l, forward[1] / l, forward[2] / l);
    let (c, s) = (math::cos(twist), math::sin(twist));
    [-kz * s + kx * ky * (1.0 - c), c + ky * ky * (1.0 - c), kx * s + kz * ky * (1.0 - c)]
}

/// HOME's vertical field of view, degrees, on the top screen's 5:3 or anything wider.
pub const HOME_FOV_DEGREES: f64 = 30.0;
/// How much of the banner plane (z = 0) HOME's view shows top to bottom, units.
pub const HOME_PLANE_HEIGHT: f64 = 24.0;
/// The height HOME's camera sits at and looks along.
pub const HOME_CENTRE_Y: f32 = 1.0;
const TOP_SCREEN_ASPECT: f32 = 5.0 / 3.0;

/// HOME's eye distance on +Z: where its view shows [`HOME_PLANE_HEIGHT`] units of the banner plane (~44.785).
pub fn home_eye_z() -> f32 {
    (HOME_PLANE_HEIGHT / 2.0 / math::tan64(math::to_radians(HOME_FOV_DEGREES / 2.0))) as f32
}

/// HOME's camera: one fixed view for every banner. An output narrower than 5:3 widens the view to keep the top screen's width.
pub fn home(aspect: f32) -> Camera {
    let ratio = TOP_SCREEN_ASPECT / aspect;
    let t = math::tan64(math::to_radians(HOME_FOV_DEGREES / 2.0)) as f32 * if ratio < 1.0 { 1.0 } else { ratio };
    let z = home_eye_z();
    Camera {
        eye: [0.0, HOME_CENTRE_Y, z],
        target: [0.0, HOME_CENTRE_Y, 0.0],
        up: [0.0, 1.0, 0.0],
        fov_y: 2.0 * math::atan(t),
        near: 1.0,
        far: z * 20.0,
        aspect: None,
        orthographic_height: 0.0,
        home_window: true,
    }
}

/// The camera a banner is framed with at `aspect`: HOME's view (a file camera does not frame it on HOME).
pub fn framing(_scene: &BannerScene, aspect: f32) -> Camera {
    home(aspect)
}

/// Where `camera` draws in a `width` x `height` output, as x, y (from the top), width, height.
pub fn viewport(camera: &Camera, width: i32, height: i32) -> [i32; 4] {
    let Some(aspect) = camera.aspect else { return [0, 0, width, height] };
    let w = round(height as f32 * aspect);
    if w <= width {
        return [(width - w) / 2, 0, w, height];
    }
    let h = round(width as f32 / aspect);
    [0, (height - h) / 2, width, h]
}

/// Kotlin's `roundToInt`: half up, saturating.
fn round(x: f32) -> i32 {
    math::floor(x + 0.5) as i32
}

/// The visible meshes' bounds (min xyz, max xyz) in the banner's first frame; `None` when nothing is visible.
pub fn bounds(scene: &BannerScene, player: &Player) -> Option<[f32; 6]> {
    let pose = player.pose(0.0, 0.0);
    let mut any = false;
    let mut b = [f32::MAX, f32::MAX, f32::MAX, -f32::MAX, -f32::MAX, -f32::MAX];
    for (mi, model) in scene.models.iter().enumerate() {
        if !model.visible {
            continue;
        }
        let bones = pose.bones_of(model, player.rest(mi));
        let skinning = pose.skinning.get(&model.name).map(|v| v.as_slice());
        for mesh in model.meshes.iter().filter(|m| m.visible) {
            let v = &*mesh.vertices;
            let mut used = alloc::vec![false; v.count];
            for &i in &mesh.indices {
                if let Some(u) = used.get_mut(i as usize) {
                    *u = true;
                }
            }
            for i in (0..v.count).filter(|&i| used[i]) {
                let p = skin::position(v, mesh, skinning, Some(bones), i);
                let p = math::transform_point(&model.world, p[0], p[1], p[2]);
                for k in 0..3 {
                    b[k] = b[k].min(p[k]);
                    b[k + 3] = b[k + 3].max(p[k]);
                }
                any = true;
            }
        }
    }
    any.then_some(b)
}

/// A camera that fills the output with the scene's first-frame bounds, looking down -Z (what unit-sized test scenes use).
pub fn fit(scene: &BannerScene, player: &Player, width: i32, height: i32) -> Camera {
    let aspect = width as f32 / height as f32;
    let b = bounds(scene, player).unwrap_or([-1.0, -1.0, -1.0, 1.0, 1.0, 1.0]);
    let (cx, cy, cz) = ((b[0] + b[3]) / 2.0, (b[1] + b[4]) / 2.0, (b[2] + b[5]) / 2.0);
    let hx = ((b[3] - b[0]) / 2.0).max(1e-3);
    let hy = ((b[4] - b[1]) / 2.0).max(1e-3);
    let hz = ((b[5] - b[2]) / 2.0).max(0.0);
    let fov = 0.35f32;
    let t = math::tan(fov / 2.0);
    let dist = (hy / t).max(hx / (t * aspect)) * 1.04 + hz;
    Camera {
        eye: [cx, cy, cz + dist],
        target: [cx, cy, cz],
        up: [0.0, 1.0, 0.0],
        fov_y: fov,
        near: (dist - hz * 2.0 - 1.0).max(dist * 0.05),
        far: dist + hz * 2.0 + 1.0,
        aspect: None,
        orthographic_height: 0.0,
        home_window: false,
    }
}

const SCREEN_W: f32 = 400.0;
const SCREEN_H: f32 = 240.0;
const WINDOW_CENTRE_X: f32 = 200.0;
const WINDOW_CENTRE_Y: f32 = 131.75;
const WINDOW_RADIUS_X: f32 = 196.0;
const WINDOW_RADIUS_Y: f32 = 103.9;

/// HOME's window in `port` (x, y from the top, width, height) of an output drawn through `camera`: centre x, centre y (from
/// the top), radius x, radius y in pixels; `None` when the camera has no window.
pub fn home_window(camera: &Camera, port: &[i32; 4]) -> Option<[f32; 4]> {
    if !camera.home_window || camera.orthographic_height > 0.0 {
        return None;
    }
    let home_tan = math::tan64(math::to_radians(HOME_FOV_DEGREES / 2.0)) as f32;
    let tan_y = math::tan(camera.fov_y / 2.0);
    let tan_x = tan_y * port[2] as f32 / port[3] as f32;
    let cx = (WINDOW_CENTRE_X / SCREEN_W * 2.0 - 1.0) * home_tan * SCREEN_W / SCREEN_H;
    let cy = (1.0 - WINDOW_CENTRE_Y / SCREEN_H * 2.0) * home_tan;
    let rx = WINDOW_RADIUS_X / SCREEN_W * 2.0 * home_tan * SCREEN_W / SCREEN_H;
    let ry = WINDOW_RADIUS_Y / SCREEN_H * 2.0 * home_tan;
    Some([
        port[0] as f32 + (cx / tan_x * 0.5 + 0.5) * port[2] as f32,
        port[1] as f32 + (0.5 - cy / tan_y * 0.5) * port[3] as f32,
        rx / tan_x * 0.5 * port[2] as f32,
        ry / tan_y * 0.5 * port[3] as f32,
    ])
}

/// How much of the pixel centred at (`x`, `y`) lies inside the window `e`, 0 to 1: the edge antialiased over one pixel.
pub fn window_coverage(e: &[f32; 4], x: f32, y: f32) -> f32 {
    let u = (x - e[0]) / e[2];
    let v = (y - e[1]) / e[3];
    let g = math::sqrt(u * u + v * v);
    if g < 1e-6 {
        return 1.0;
    }
    let grad = math::sqrt(u * u / (e[2] * e[2]) + v * v / (e[3] * e[3])) / g;
    math::clamp(0.5 - (g - 1.0) / grad, 0.0, 1.0)
}
