// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Nintendo 3DS HOME-menu banners as a renderer-neutral scene.
//!
//! A 3DS game's ExeFS `banner` is a CBMD: a common CGFX (models, textures, lights, animations), optional per-language
//! CGFX files whose textures replace the common file's placeholders, and a jingle this crate never reads. [`Cbmd`]
//! splits the container, [`cgfx`] parses a CGFX into a [`BannerScene`], and [`dump`] writes any scene as deterministic
//! JSON (what the parity tests compare against the original implementation).
//!
//! Nothing here draws, opens files or decrypts anything. Parsing never panics on hostile input.
//!
//! ```
//! assert!(banner3ds_scene::Cbmd::parse(&[0; 0x90]).is_none());
//! ```
#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod animations;
pub mod bcwav;
pub mod cbmd;
pub mod cgfx;
pub mod draw;
pub mod dump;
pub mod lz11;
mod model;
mod pica_commands;
pub mod pica_textures;
mod reader;
pub mod source;

pub use cbmd::Cbmd;
pub use model::*;
pub use reader::FormatError;
