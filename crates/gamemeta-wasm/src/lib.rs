// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The readers and both banner renderers for the browser, through wasm-bindgen.
//!
//! Meant for a Web Worker: a picked `File` is read in place (`FileReaderSync` over `Blob.slice`, see [`source`]), so a
//! multi-gigabyte ROM is never loaded whole. [`inspect`] gives the metadata and ids as JSON plus icons; the RetroAchievements
//! hash is fed chunk by chunk through [`RaHash`]; a banner is a [`Scene`] drawn by the CPU renderer ([`SoftRenderer`]) or by
//! the GLES renderer in a WebGL 2 context the page made ([`Gles`], through the JS shim in `js/gl.js`). The library itself
//! still makes no context and opens no file.

use std::fmt::Write;
use std::mem::ManuallyDrop;
use std::rc::Rc;

use banner3ds_scene::BannerScene;
use banner3ds_scene::bcwav::Sound;
use banner3ds_scene::source::{self as scene_source, SceneError};
use gamemeta_core::ids::{self, RaHasher};
use gamemeta_core::{GameInfo, RgbaImage};
use gamemeta_core::{nds, wiiu};
use js_sys::{Array, Function};
use wasm_bindgen::prelude::*;

mod export;
mod folder;
mod gl;
mod source;

use source::{BlobSource, JsOpener};

/// What [`inspect`] found in one file or folder.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Inspection {
    json: String,
    icon: Option<RgbaImage>,
    small_icon: Option<RgbaImage>,
    scene: Option<Scene>,
}

#[wasm_bindgen]
impl Inspection {
    /// `{name, size, known, format, info, ids, ra, banner, zip?}` as JSON (`known`: a reader exists for this file name; `zip`: the ROM read and the zip's others): `info` and `ids` are the CLI's objects (`null` when the file
    /// gives none), `ra` says whether a RetroAchievements hash applies, `banner` is `{state, ...}` (see `banner_json`).
    pub fn json(&self) -> String {
        self.json.clone()
    }

    /// The large icon's RGBA bytes (its size is in `info.icon`), if any.
    pub fn icon_rgba(&self) -> Option<Vec<u8>> {
        self.icon.as_ref().map(|i| i.pixels.clone())
    }

    /// The small icon's RGBA bytes (its size is in `info.smallIcon`), if any.
    pub fn small_icon_rgba(&self) -> Option<Vec<u8>> {
        self.small_icon.as_ref().map(|i| i.pixels.clone())
    }

    /// The banner scene, once (`None` when the file has none or it was already taken).
    pub fn take_scene(&mut self) -> Option<Scene> {
        self.scene.take()
    }
}

/// Reads the file `blob` (called `name`) as the library would on any host: the metadata, the ids (not the RetroAchievements
/// hash, which reads the whole file: [`RaHash`]) and the 3DS banner scene. `open` (optional) maps the path of a file beside
/// it (a cue sheet's track) to a `Blob` or `undefined`. For a zip, `entry` names the ROM inside it to read (the first ROM when
/// absent); the result's `zip` object lists the zip's other ROMs so the page can ask for each.
#[wasm_bindgen]
pub fn inspect(blob: JsValue, name: &str, open: Option<Function>, entry: Option<String>) -> Inspection {
    let mut source = BlobSource::new(blob);
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e).to_ascii_lowercase();
    let size = gamemeta_core::ByteSource::size(&source);
    let mut zip = None;
    let info = if ext == "zip" {
        let roms = nds::zip_roms(&mut source);
        let chosen = entry.as_deref().and_then(|e| roms.iter().find(|r| r.name == e)).or(roms.first()).cloned();
        let info = chosen.as_ref().and_then(|r| nds::read_zip_info_of(&mut source, &r.name));
        zip = chosen.map(|rom| {
            let others = if entry.is_some() {
                Vec::new()
            } else {
                roms.iter().filter(|r| r.name != rom.name).map(|r| r.name.clone()).collect()
            };
            ZipJson { rom, others }
        });
        info
    } else {
        gamemeta_core::read(&mut source, name)
    };
    let mut ids = ids::read_with(&mut source, name, &mut JsOpener(open.as_ref()));
    if let Some(z) = &zip {
        let ids = ids.get_or_insert_with(Default::default);
        ids.zip_crc = Some(format!("{:08x}", z.rom.crc));
        ids.zip_size = Some(z.rom.size);
    }
    let scene = match ext.as_str() {
        "3ds" | "cci" | "cxi" | "cia" | "app" | "cbmd" | "cgfx" => Some(scene_source::read_with_sound(&mut source)),
        _ => None,
    };
    let ra = RaHasher::for_file(name, Some(size)).is_some();
    assemble(name, size, info, ids, ra, scene, zip)
}

/// The ROM a zip's card shows, and the names of the zip's other ROMs.
struct ZipJson {
    rom: nds::ZipRom,
    others: Vec<String>,
}

/// Reads a picked folder as a Wii U title: `paths` are the files' paths relative to the folder's parent (`webkitRelativePath`),
/// `blobs` the matching `File`s. `format` is `WiiUTitle` when a `meta/meta.xml` is among them, else everything is empty.
#[wasm_bindgen]
pub fn inspect_folder(paths: Vec<String>, blobs: Array, name: &str) -> Inspection {
    let blobs: Vec<JsValue> = blobs.iter().collect();
    let (info, ids) = match folder::FolderFiles::new(paths, blobs) {
        Some(mut files) => {
            let id = wiiu::title_id(&mut files);
            (wiiu::read_title(&mut files), id.map(|t| ids::Ids { title_id: Some(t), ..Default::default() }))
        }
        None => (None, None),
    };
    assemble(name, 0, info, ids, false, None, None)
}

fn assemble(
    name: &str,
    size: u64,
    info: Option<GameInfo>,
    ids: Option<ids::Ids>,
    ra: bool,
    scene: Option<Result<(BannerScene, Option<Sound>), SceneError>>,
    zip: Option<ZipJson>,
) -> Inspection {
    let (banner, scene) = match scene {
        None => ("{\"state\":\"none\"}".to_string(), None),
        Some(Ok((scene, sound))) => {
            let scene = Scene::new(scene, sound);
            (banner_json(&scene), Some(scene))
        }
        Some(Err(e)) => (format!("{{\"state\":\"{}\"}}", scene_error(e)), None),
    };
    let mut json = String::from("{\"name\":");
    gamemeta_json::string(&mut json, name);
    let _ = write!(
        json,
        ",\"size\":{size},\"known\":{},\"format\":{},\"info\":{},\"ids\":{},\"ra\":{ra},\"banner\":{banner}",
        is_known(name) || ra,
        info.as_ref().map_or("null".to_string(), |i| format!("\"{:?}\"", i.platform)),
        gamemeta_json::info(info.as_ref()),
        gamemeta_json::ids(ids.as_ref()),
    );
    if let Some(z) = &zip {
        json.push_str(",\"zip\":{\"entry\":");
        gamemeta_json::string(&mut json, &z.rom.name);
        json.push_str(",\"others\":[");
        for (i, other) in z.others.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            gamemeta_json::string(&mut json, other);
        }
        json.push_str("]}");
    }
    json.push('}');
    let (icon, small_icon) = info.map_or((None, None), |i| (i.icon, i.small_icon));
    Inspection { json, icon, small_icon, scene }
}

/// Whether the library has a reader for files called `name`: such a file that gives nothing is damaged, not unsupported.
fn is_known(name: &str) -> bool {
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e).to_ascii_lowercase();
    gamemeta_core::format_for(name).is_some()
        || matches!(
            ext.as_str(),
            "iso" | "bin" | "img" | "cue" | "pbp" | "chd" | "nsp" | "xbe" | "cbmd" | "cgfx" | "3dsx"
        )
}

fn scene_error(e: SceneError) -> &'static str {
    match e {
        SceneError::NotFound => "missing",
        SceneError::Encrypted => "encrypted",
        SceneError::Damaged => "damaged",
    }
}

/// `{"state":"ok",...}` with what the scene holds.
fn banner_json(scene: &Scene) -> String {
    let s = &scene.scene;
    format!(
        "{{\"state\":\"ok\",\"frames\":{},\"animated\":{},\"sound\":{},\"models\":{},\"meshes\":{},\"textures\":{},\"unsupported\":{}}}",
        scene.frames(),
        scene.animated(),
        scene.has_sound(),
        s.models.len(),
        s.models.iter().map(|m| m.meshes.len()).sum::<usize>(),
        s.textures.len(),
        s.unsupported.len(),
    )
}

/// The RetroAchievements hash of one file, fed in order: `RaHash.new(name, size)`, `update` with each chunk, `finish`.
#[wasm_bindgen]
#[derive(Debug)]
pub struct RaHash {
    hasher: Option<RaHasher>,
}

#[wasm_bindgen]
impl RaHash {
    /// A hasher for `name` of `size` bytes, or `None` when the file's platform is not hashed.
    pub fn start(name: &str, size: f64) -> Option<RaHash> {
        RaHasher::for_file(name, Some(size as u64)).map(|h| RaHash { hasher: Some(h) })
    }

    /// The next bytes of the file.
    pub fn update(&mut self, chunk: &[u8]) {
        if let Some(h) = self.hasher.as_mut() {
            h.update(chunk);
        }
    }

    /// The lower-case hex MD5, or `None` when the file is not hashable this way. Once.
    pub fn finish(&mut self) -> Option<String> {
        self.hasher.take()?.finish()
    }
}

/// A banner scene, ready to draw.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Scene {
    scene: Rc<BannerScene>,
    sound: Option<Sound>,
}

impl Scene {
    fn new(scene: BannerScene, sound: Option<Sound>) -> Scene {
        Scene { scene: Rc::new(scene), sound }
    }
}

#[wasm_bindgen]
impl Scene {
    /// The loop length in frames (60 to the second); 0 when the banner does not animate.
    pub fn frames(&self) -> f32 {
        banner3ds_scene::draw::Player::new(&self.scene).frames
    }

    /// Whether anything moves.
    pub fn animated(&self) -> bool {
        self.frames() > 0.0
    }

    /// Whether the banner has a jingle that decodes.
    pub fn has_sound(&self) -> bool {
        self.sound.is_some()
    }

    /// The jingle's sample rate (0 when there is none).
    pub fn sound_rate(&self) -> u32 {
        self.sound.as_ref().map_or(0, |s| s.sample_rate)
    }

    /// The jingle's channel count (0 when there is none).
    pub fn sound_channels(&self) -> u8 {
        self.sound.as_ref().map_or(0, |s| s.channels)
    }

    /// The jingle as interleaved 16-bit samples at `rate` Hz (the file's own rate when `rate` is 0), cut to `seconds`
    /// (no cut when negative); empty when there is none.
    pub fn sound_pcm(&self, rate: u32, seconds: f64) -> Vec<i16> {
        let Some(s) = &self.sound else { return Vec::new() };
        let channels = usize::from(s.channels);
        let pcm = banner3ds_export::audio::resample(
            &s.pcm,
            channels,
            s.sample_rate,
            if rate == 0 { s.sample_rate } else { rate },
        );
        let at = if rate == 0 { s.sample_rate } else { rate };
        if seconds < 0.0 { pcm } else { banner3ds_export::audio::trim(&pcm, channels, at, seconds).to_vec() }
    }
}

/// The CPU renderer over a scene: `render(frame, width, height)` gives `width * height` RGBA bytes, top row first.
#[wasm_bindgen]
#[derive(Debug)]
pub struct SoftRenderer {
    // Declared first: it borrows `_scene`'s allocation, which the `Rc` keeps alive for as long as this struct lives.
    renderer: banner3ds_soft::Renderer<'static>,
    _scene: Rc<BannerScene>,
}

#[wasm_bindgen]
impl SoftRenderer {
    #[wasm_bindgen(constructor)]
    pub fn new(scene: &Scene) -> SoftRenderer {
        let rc = Rc::clone(&scene.scene);
        // SAFETY: the renderer borrows the `BannerScene` inside `rc`, whose heap allocation does not move and is kept alive by
        // `_scene` in the same struct, dropped after the renderer.
        let renderer = unsafe {
            std::mem::transmute::<banner3ds_soft::Renderer<'_>, banner3ds_soft::Renderer<'static>>(
                banner3ds_soft::Renderer::new(&rc),
            )
        };
        SoftRenderer { renderer, _scene: rc }
    }

    /// The banner at `frame` (60 to the second), `width` x `height`, through HOME's view over transparent black.
    pub fn render(&self, frame: f32, width: u32, height: u32) -> Result<Vec<u8>, JsError> {
        let n = u64::from(width) * u64::from(height);
        if n == 0 || n > banner3ds_soft::MAX_PIXELS {
            return Err(JsError::new("bad size"));
        }
        let mut out = vec![0u8; n as usize * 4];
        self.renderer.render(frame, width, height, &mut out).map_err(|e| JsError::new(&format!("{e:?}")))?;
        Ok(out)
    }
}

/// The GLES renderer over a scene in one WebGL 2 context. Every method takes that context and makes it current first;
/// [`destroy`](Gles::destroy) must be called (with the context) before the object is dropped, or its GL objects leak.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Gles {
    renderer: Option<ManuallyDrop<banner3ds_gles::Renderer<'static>>>,
    _scene: Rc<BannerScene>,
}

#[wasm_bindgen]
impl Gles {
    /// Sets up programs, buffers and textures for `scene` in `gl` (a `WebGL2RenderingContext`); an error when nothing in the
    /// scene can be drawn.
    #[wasm_bindgen(constructor)]
    pub fn new(scene: &Scene, gl: &JsValue) -> Result<Gles, JsError> {
        gl::set_context(gl);
        let rc = Rc::clone(&scene.scene);
        // SAFETY: the context is current (just made so) and the shim's functions are the GL table; the renderer borrows the
        // scene inside `rc`, kept alive by `_scene` in the same struct (see `SoftRenderer::new`).
        let renderer =
            unsafe { banner3ds_gles::Renderer::new(&rc, gl::table()) }.map_err(|e| JsError::new(&format!("{e:?}")))?;
        // SAFETY: as above, the borrowed scene outlives the renderer.
        let renderer =
            unsafe { std::mem::transmute::<banner3ds_gles::Renderer<'_>, banner3ds_gles::Renderer<'static>>(renderer) };
        Ok(Gles { renderer: Some(ManuallyDrop::new(renderer)), _scene: rc })
    }

    /// Draws the banner at `frame` (60 to the second) into the whole `width` x `height` drawing buffer of `gl`, clearing it to
    /// transparent black first.
    pub fn draw(&mut self, gl: &JsValue, frame: f32, width: i32, height: i32) -> Result<(), JsError> {
        let r = self.renderer.as_mut().ok_or_else(|| JsError::new("destroyed"))?;
        if width <= 0 || height <= 0 {
            return Err(JsError::new("bad size"));
        }
        gl::set_context(gl);
        // SAFETY: `gl` is current (just made so) and is the context this renderer was made in.
        unsafe { r.draw(frame, banner3ds_gles::Viewport::of(width, height)) };
        Ok(())
    }

    /// Deletes the GL objects. Once; later calls do nothing.
    pub fn destroy(&mut self, gl: &JsValue) {
        if let Some(mut r) = self.renderer.take() {
            gl::set_context(gl);
            // SAFETY: taken once, in the context it was made in, which is current.
            unsafe { ManuallyDrop::take(&mut r).destroy() };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::CString;

    use banner3ds_gles::Gl;

    /// Every entry point the renderer asks for has a shim, and the JS side defines a function of that field's name.
    #[test]
    fn the_shim_covers_the_renderers_table_and_the_js_defines_each_call() {
        for name in Gl::NAMES {
            assert!(!crate::gl::lookup(name).is_null(), "no shim for {name:?}");
        }
        assert!(crate::gl::lookup(&CString::new("glNotAThing").unwrap()).is_null());
        let js = include_str!("../js/gl.js");
        let rust = include_str!("gl.rs");
        let fields = rust.lines().filter_map(|l| l.trim().split_once(" = c\"gl").map(|(f, _)| f.to_string()));
        let mut n = 0;
        for field in fields {
            assert!(js.contains(&format!("export function {field}(")), "gl.js lacks {field}");
            n += 1;
        }
        assert_eq!(n, Gl::NAMES.len());
    }
}
