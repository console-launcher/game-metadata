// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What both banner renderers share to draw a frame, on the CPU: the cameras and HOME's framing ([`camera`]), the clips sampled
//! into a pose with HOME's turntable ([`pose`]), skinning ([`skin`]), bone billboards ([`billboard`]), the draw order
//! ([`order`]) and the vertex lighting and fragment-lighting colour registers ([`light`]). No drawing happens here.

pub mod billboard;
pub mod camera;
pub mod light;
pub mod math;
pub mod order;
pub mod pose;
pub mod skin;

pub use camera::Camera;
pub use pose::{Player, Pose};
