// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A scene from a file: a bare CBMD or CGFX, or a 3DS file whose ExeFS holds the banner.
//!
//! Only decrypted content reads; an encrypted 3DS file is [`SceneError::Encrypted`] and nothing is ever decrypted.

use crate::bcwav::{self, Sound};
use crate::{BannerScene, Cbmd, cgfx};
use alloc::vec;
use gamemeta_core::threeds::{self, Reason};
use gamemeta_core::{ByteSource, ReadError};

/// The most a bare CBMD or CGFX file may take (a banner is at most a few hundred KB; the ExeFS reader caps its own).
const MAX_FILE: u64 = 16 << 20;
const MAGIC_CBMD: &[u8; 4] = b"CBMD";
const MAGIC_CGFX: &[u8; 4] = b"CGFX";

/// Why a file gives no scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneError {
    /// Not a banner, or a banner that does not parse, or a 3DS file without one.
    NotFound,
    /// A 3DS file with encrypted content.
    Encrypted,
    /// Truncated or inconsistent headers, or the source failed.
    Damaged,
}

/// The scene of `source`: a bare CBMD (or CGFX) is parsed as it is, anything else is read as a 3DS file (NCSD, CIA or NCCH).
pub fn read<S: ByteSource + ?Sized>(source: &mut S) -> Result<BannerScene, SceneError> {
    read_with_sound(source).map(|(scene, _)| scene)
}

/// [`read`], and the banner's jingle when it has one that decodes (see [`crate::bcwav`]).
pub fn read_with_sound<S: ByteSource + ?Sized>(source: &mut S) -> Result<(BannerScene, Option<Sound>), SceneError> {
    let size = source.size();
    let mut magic = [0u8; 4];
    if size >= 4 {
        source.read_at(0, &mut magic).map_err(|_| SceneError::Damaged)?;
    }
    if &magic == MAGIC_CBMD || &magic == MAGIC_CGFX {
        if size > MAX_FILE {
            return Err(SceneError::NotFound);
        }
        let mut bytes = vec![0u8; size as usize];
        source.read_at(0, &mut bytes).map_err(|e| match e {
            ReadError::OutOfRange | ReadError::Io => SceneError::Damaged,
        })?;
        return from_banner_with_sound(&bytes);
    }
    let found = threeds::read(source).map_err(|reason| match reason {
        Reason::Encrypted => SceneError::Encrypted,
        Reason::Damaged => SceneError::Damaged,
        Reason::NotA3dsFile | Reason::NoBannerInExeFs => SceneError::NotFound,
    })?;
    from_banner_with_sound(&found.banner)
}

/// The scene of the bytes of a CBMD or CGFX file.
pub fn from_banner(bytes: &[u8]) -> Result<BannerScene, SceneError> {
    let scene =
        if bytes.starts_with(MAGIC_CGFX) { cgfx::parse(bytes) } else { Cbmd::parse(bytes).and_then(|c| c.scene()) };
    scene.ok_or(SceneError::NotFound)
}

/// [`from_banner`] and the jingle (a CGFX alone has none).
pub fn from_banner_with_sound(bytes: &[u8]) -> Result<(BannerScene, Option<Sound>), SceneError> {
    if bytes.starts_with(MAGIC_CGFX) {
        return cgfx::parse(bytes).map(|s| (s, None)).ok_or(SceneError::NotFound);
    }
    let cbmd = Cbmd::parse(bytes).ok_or(SceneError::NotFound)?;
    let scene = cbmd.scene().ok_or(SceneError::NotFound)?;
    Ok((scene, cbmd.jingle.as_deref().and_then(bcwav::decode)))
}
