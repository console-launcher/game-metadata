// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `gamemeta <file>...`: one JSON object per file, one per line (`null` for a file nothing reads). A Wii U `.rpx` is
//! read through the title folders around it (`../meta/meta.xml`).
//!
//! `gamemeta ids <file>...`: the ids of each file (serial, game id, title id, CHD and zip checksums, PSP and Xbox ids and the
//! RetroAchievements MD5) as JSON, one line per file (`null` when it has none). Files beside it (a cue sheet's tracks, a
//! `.rpx`'s `meta/meta.xml`) are read from its folder.
//!
//! `gamemeta formats`: what is supported, one line per platform: its name, an em dash and its extensions (`.nds .dsi ...`).
//! The same list the website shows.
//!
//! `gamemeta scene <file>...`: the 3DS banner of each file (a CBMD or CGFX, or a decrypted `.3ds`, `.cia`, `.cxi`) as scene
//! JSON, one line per file (`null` when there is no banner scene).
//!
//! `gamemeta wii-banner <file> [--frame N]... [--poster] [--scale S] [-o <out.png>] [--bin <out.bin>]`: the Wii channel banner of
//! an RVZ disc, an `opening.bnr` or a `banner.bin`. Prints one JSON line (animation lengths, size, panes skipped, textures
//! missing); `-o` writes the frames asked for (60 to the second, 0 and scale 1 by default; `--poster` is the settled banner) as
//! RGBA PNGs, `--bin` the archive's own bytes. With several frames `-o a.png` writes `a-<frame>.png` and `a-poster.png`.
//!
//! `gamemeta render <file> [--t <seconds>] [--size <W>x<H>] -o <out.png>`: one frame of that banner (time 0 and 400 x 240,
//! HOME's top screen, by default) drawn by the CPU renderer through HOME's view over transparent black, as an RGBA PNG.

use std::fs::File;
use std::io::Write;
use std::process::ExitCode;

use gamemeta_core::{ByteSource, ReadError};

use gamemeta_json as json;

/// Positioned reads over a file.
struct FileSource {
    file: File,
    size: u64,
}

impl ByteSource for FileSource {
    fn size(&self) -> u64 {
        self.size
    }
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        use std::io::{Read, Seek, SeekFrom};
        self.file.seek(SeekFrom::Start(offset)).map_err(|_| ReadError::Io)?;
        self.file.read_exact(buf).map_err(|e| match e.kind() {
            std::io::ErrorKind::UnexpectedEof => ReadError::OutOfRange,
            _ => ReadError::Io,
        })
    }
}

/// A Wii U title from its `.rpx`: the title's files are the folders beside the `code/` folder the `.rpx` sits in.
fn read_rpx(path: &str) -> Option<gamemeta_core::GameInfo> {
    let dir = std::path::Path::new(path).parent()?.to_path_buf();
    let open = move |name: &str| -> Option<Box<dyn ByteSource>> {
        let file = File::open(dir.join(name)).ok()?;
        let size = file.metadata().ok()?.len();
        Some(Box::new(FileSource { file, size }))
    };
    gamemeta_core::wiiu::read_title(&mut gamemeta_core::wiiu::OpenerFiles(open))
}

/// The ids of the file at `path`, with the RetroAchievements MD5 (which reads the whole file) filled in where there is one.
fn ids_line(path: &str, source: &mut FileSource) -> String {
    let dir = std::path::Path::new(path).parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let mut opener = move |name: &str| -> Option<Box<dyn ByteSource>> {
        let file = File::open(dir.join(name)).ok()?;
        let size = file.metadata().ok()?.len();
        Some(Box::new(FileSource { file, size }))
    };
    let mut ids = gamemeta_core::ids::read_with(source, path, &mut opener);
    if let Some(hash) = gamemeta_core::ids::ra_hash(source, path) {
        ids.get_or_insert_with(Default::default).ra_md5 = Some(hash);
    }
    json::ids(ids.as_ref())
}

/// The scene JSON of the banner in `source`, `null` when there is none.
fn scene_line(source: &mut FileSource) -> String {
    match banner3ds_scene::source::read(source) {
        Ok(scene) => banner3ds_scene::dump::scene_json(&scene),
        Err(_) => "null".to_string(),
    }
}

/// What `gamemeta render` draws.
#[derive(Debug, PartialEq)]
struct RenderArgs {
    path: String,
    seconds: f32,
    width: u32,
    height: u32,
    out: String,
}

const RENDER_USAGE: &str =
    "usage: gamemeta render <banner.cbmd|.3ds|.cia> [--t <seconds>] [--size <W>x<H>] -o <out.png>";

/// Parses `render`'s arguments (after the word `render`): one file, `-o`, optional `--t` and `--size`.
fn render_args(args: &[String]) -> Result<RenderArgs, String> {
    let usage = || RENDER_USAGE.to_string();
    let (mut path, mut out, mut seconds, mut size) = (None, None, 0.0f32, (400u32, 240u32));
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--t" => {
                seconds = it.next().and_then(|v| v.parse().ok()).filter(|t: &f32| t.is_finite()).ok_or_else(usage)?
            }
            "--size" => {
                let v = it.next().ok_or_else(usage)?;
                let (w, h) = v.split_once('x').ok_or_else(usage)?;
                size = (w.parse().map_err(|_| usage())?, h.parse().map_err(|_| usage())?);
                let pixels = size.0 as u64 * size.1 as u64;
                if pixels == 0 || pixels > banner3ds_soft::MAX_PIXELS {
                    return Err(usage());
                }
            }
            "-o" => out = Some(it.next().ok_or_else(usage)?.clone()),
            flag if flag.starts_with('-') => return Err(usage()),
            file if path.is_none() => path = Some(file.to_string()),
            _ => return Err(usage()),
        }
    }
    let (Some(path), Some(out)) = (path, out) else { return Err(usage()) };
    Ok(RenderArgs { path, seconds, width: size.0, height: size.1, out })
}

/// Draws the frame `args` asks for (60 frames to the second) and writes the PNG.
fn render_file(args: &RenderArgs) -> Result<(), String> {
    let file = File::open(&args.path).map_err(|e| format!("{}: {e}", args.path))?;
    let size = file.metadata().map_err(|e| format!("{}: {e}", args.path))?.len();
    let scene = banner3ds_scene::source::read(&mut FileSource { file, size })
        .map_err(|e| format!("{}: no banner scene ({e:?})", args.path))?;
    let mut pixels = vec![0u8; args.width as usize * args.height as usize * 4];
    banner3ds_soft::render(&scene, args.seconds * 60.0, args.width, args.height, &mut pixels)
        .map_err(|e| format!("render: {e:?}"))?;
    let image = gamemeta_core::RgbaImage { width: args.width, height: args.height, pixels };
    let png = gamemeta_core::png::encode(&image).ok_or("png: bad image")?;
    std::fs::write(&args.out, png).map_err(|e| format!("{}: {e}", args.out))
}

/// What `gamemeta banner3ds` does.
#[derive(Debug, PartialEq)]
struct ExportArgs {
    path: String,
    format: banner3ds_export::render::Format,
    scale: u32,
    out: String,
}

const EXPORT_USAGE: &str = "usage: gamemeta banner3ds <banner.cbmd|.3ds|.cia> --gif|--webp [--scale 1|2|3] -o <out.gif|out.webp>   (MP4 is made by the website: it needs the browser's video encoder)";

fn export_args(args: &[String]) -> Result<ExportArgs, String> {
    use banner3ds_export::render::Format;
    let usage = || EXPORT_USAGE.to_string();
    let (mut path, mut out, mut format, mut scale) = (None, None, None, 1u32);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--gif" | "--webp" if format.is_some() => return Err(usage()),
            "--gif" => format = Some(Format::Gif),
            "--webp" => format = Some(Format::Webp),
            "--mp4" => {
                return Err(format!(
                    "MP4 is only made by the website (it needs the browser's video encoder). {EXPORT_USAGE}"
                ));
            }
            "--scale" => scale = it.next().and_then(|v| v.parse().ok()).ok_or_else(usage)?,
            "-o" => out = Some(it.next().ok_or_else(usage)?.clone()),
            flag if flag.starts_with('-') => return Err(usage()),
            file if path.is_none() => path = Some(file.to_string()),
            _ => return Err(usage()),
        }
    }
    let (Some(path), Some(out), Some(format)) = (path, out, format) else { return Err(usage()) };
    if !(1..=banner3ds_export::render::MAX_SCALE).contains(&scale) {
        return Err(usage());
    }
    Ok(ExportArgs { path, format, scale, out })
}

/// Draws the banner's animation (the intro once, then one loop) and writes the GIF or WebP; prints a JSON summary.
fn export_file(args: &ExportArgs) -> Result<(), String> {
    let file = File::open(&args.path).map_err(|e| format!("{}: {e}", args.path))?;
    let size = file.metadata().map_err(|e| format!("{}: {e}", args.path))?.len();
    let scene = banner3ds_scene::source::read(&mut FileSource { file, size })
        .map_err(|e| format!("{}: no banner scene ({e:?})", args.path))?;
    let mut pictures = 0;
    let bytes = banner3ds_export::render::encode(&scene, args.format, args.scale, |done, _| {
        pictures = done;
        true
    })
    .map_err(|e| format!("export: {e}"))?;
    std::fs::write(&args.out, &bytes).map_err(|e| format!("{}: {e}", args.out))?;
    let kind = if args.format == banner3ds_export::render::Format::Gif { "gif" } else { "webp" };
    println!("{{\"out\":{:?},\"format\":\"{kind}\",\"pictures\":{pictures},\"bytes\":{}}}", args.out, bytes.len());
    Ok(())
}

/// The verb a command line starts with (`scene`, `ids`, `info`; `info` is what no verb means) and the paths after it.
fn command_of(args: &[String]) -> (Option<String>, Vec<String>) {
    match args.first().filter(|a| matches!(a.as_str(), "scene" | "ids" | "info")) {
        Some(verb) => (Some(verb.clone()), args[1..].to_vec()),
        None => (None, args.to_vec()),
    }
}

/// What `gamemeta wii-banner` does.
#[derive(Debug, PartialEq)]
struct WiiBannerArgs {
    path: String,
    /// Frames asked for, as written (`None` is the poster).
    frames: Vec<Option<String>>,
    scale: f32,
    out: Option<String>,
    bin: Option<String>,
    /// Read the disc's `meta/icon.bin` (the menu's grid tile) instead of its banner; an RVZ only.
    icon: bool,
}

const WII_BANNER_USAGE: &str = "usage: gamemeta wii-banner <file.rvz|opening.bnr|banner.bin> [--frame N]... [--poster] [--scale S] [-o <out.png>] [--bin <out.bin>] [--icon]";

fn wii_banner_args(args: &[String]) -> Result<WiiBannerArgs, String> {
    let usage = || WII_BANNER_USAGE.to_string();
    let (mut path, mut out, mut bin, mut scale, mut frames, mut icon) = (None, None, None, 1.0f32, Vec::new(), false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--frame" => {
                let v = it.next().ok_or_else(usage)?;
                v.parse::<f32>().ok().filter(|f| f.is_finite()).ok_or_else(usage)?;
                frames.push(Some(v.clone()));
            }
            "--poster" => frames.push(None),
            "--scale" => {
                scale = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|s: &f32| s.is_finite() && *s > 0.0)
                    .ok_or_else(usage)?
            }
            "-o" => out = Some(it.next().ok_or_else(usage)?.clone()),
            "--bin" => bin = Some(it.next().ok_or_else(usage)?.clone()),
            "--icon" => icon = true,
            flag if flag.starts_with('-') => return Err(usage()),
            file if path.is_none() => path = Some(file.to_string()),
            _ => return Err(usage()),
        }
    }
    let Some(path) = path else { return Err(usage()) };
    if out.is_some() && frames.is_empty() {
        frames.push(Some("0".into()));
    }
    if out.is_none() && !frames.is_empty() {
        return Err(usage()); // frames to draw and nowhere to write them
    }
    Ok(WiiBannerArgs { path, frames, scale, out, bin, icon })
}

/// The PNG paths and frames (`None`: the poster) to write: `out` itself for one frame, else `out` with `-<frame>` or `-poster`
/// before its extension.
fn wii_banner_outputs(out: &str, frames: &[Option<String>]) -> Vec<(String, Option<String>)> {
    if let [only] = frames {
        return vec![(out.to_string(), only.clone())];
    }
    let name_at = out.rfind('/').map_or(0, |i| i + 1);
    let (stem, ext) = match out[name_at..].rfind('.').filter(|&i| i > 0) {
        Some(i) => (&out[..name_at + i], &out[name_at + i + 1..]),
        None => (out, ""),
    };
    frames
        .iter()
        .map(|f| {
            let label = f.as_deref().unwrap_or("poster");
            (if ext.is_empty() { format!("{stem}-{label}") } else { format!("{stem}-{label}.{ext}") }, f.clone())
        })
        .collect()
}

/// Opens the banner, prints its summary line, and writes what `args` asks for.
fn wii_banner_file(args: &WiiBannerArgs) -> Result<(), String> {
    let file = File::open(&args.path).map_err(|e| format!("{}: {e}", args.path))?;
    let size = file.metadata().map_err(|e| format!("{}: {e}", args.path))?.len();
    let mut source = FileSource { file, size };
    let archive = if args.icon {
        gamemeta_core::wii::channel_icon_bin(&mut source)
    } else {
        gamemeta_core::wii::channel_banner_archive(&mut source)
    }
    .map_err(|e| format!("{}: {e:?}", args.path))?
    .ok_or_else(|| format!("{}: no channel banner", args.path))?;
    let banner = gamemeta_core::wii::banner::WiiBanner::open(&archive)
        .ok_or_else(|| format!("{}: no channel banner", args.path))?;
    if let Some(bin) = &args.bin {
        std::fs::write(bin, &archive).map_err(|e| format!("{bin}: {e}"))?;
    }
    if let Some(out) = &args.out {
        for (path, frame) in wii_banner_outputs(out, &args.frames) {
            let frame = frame.map_or(banner.start_frames() as f32, |f| f.parse().unwrap_or(0.0));
            let image = banner.render(frame, args.scale).ok_or("render: the scale gives no usable size")?;
            let png = gamemeta_core::png::encode(&image).ok_or("png: bad image")?;
            std::fs::write(&path, png).map_err(|e| format!("{path}: {e}"))?;
        }
    }
    println!("{}", json::wii_banner(&banner));
    Ok(())
}

/// The supported formats, a line each: `Platform — .ext .ext`.
fn formats_text() -> String {
    gamemeta_core::supported_formats()
        .iter()
        .map(|s| {
            format!("{} — {}\n", s.platform, s.extensions.iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(" "))
        })
        .collect()
}

fn main() -> ExitCode {
    let mut paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.first().is_some_and(|p| p == "formats") {
        print!("{}", formats_text());
        return ExitCode::SUCCESS;
    }
    if paths.first().is_some_and(|p| p == "wii-banner") {
        return match wii_banner_args(&paths[1..]).and_then(|a| wii_banner_file(&a)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("gamemeta: {e}");
                ExitCode::from(2)
            }
        };
    }
    if paths.first().is_some_and(|p| p == "banner3ds") {
        return match export_args(&paths[1..]).and_then(|a| export_file(&a)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("gamemeta: {e}");
                ExitCode::from(2)
            }
        };
    }
    if paths.first().is_some_and(|p| p == "render") {
        return match render_args(&paths[1..]).and_then(|a| render_file(&a)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("gamemeta: {e}");
                ExitCode::from(2)
            }
        };
    }
    let command;
    (command, paths) = command_of(&paths);
    let (scene, ids) = (command.as_deref() == Some("scene"), command.as_deref() == Some("ids"));
    if paths.is_empty() || paths.iter().any(|p| p == "-h" || p == "--help") {
        eprintln!(
            "usage: gamemeta [info|scene|ids] <file>...   |   gamemeta formats   (one JSON object per file, per line; `info` (the default): titles, ids, regions, version and, for a cartridge ROM, its typed header; `scene`: the 3DS banner scene; `ids`: serials, ids and hashes)\n       {RENDER_USAGE}\n       {WII_BANNER_USAGE}\n       {EXPORT_USAGE}"
        );
        return ExitCode::from(2);
    }
    let mut out = std::io::stdout().lock();
    let mut failed = false;
    for path in &paths {
        let line = match File::open(path).and_then(|f| f.metadata().map(|m| (f, m.len()))) {
            Ok((file, size)) if scene => scene_line(&mut FileSource { file, size }),
            Ok((file, size)) if ids => ids_line(path, &mut FileSource { file, size }),
            Ok((file, size)) => {
                let info = if gamemeta_core::wiiu::is_rpx(path) {
                    read_rpx(path)
                } else {
                    gamemeta_core::read(&mut FileSource { file, size }, path)
                };
                json::info(info.as_ref())
            }
            Err(e) => {
                eprintln!("gamemeta: {path}: {e}");
                failed = true;
                continue;
            }
        };
        let _ = writeln!(out, "{line}");
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_lists_a_line_per_platform_with_dotted_extensions() {
        let text = formats_text();
        assert!(text.lines().any(|l| l == "Nintendo DS, DSi — .nds .dsi .srl .ids .zip"), "{text}");
        assert!(text.lines().any(|l| l.starts_with("Game Boy Advance — .gba")), "{text}");
        assert!(text.lines().all(|l| l.contains(" — .")), "{text}");
    }

    #[test]
    fn a_banner_file_prints_its_scene_and_other_files_print_null() {
        let dir = std::env::temp_dir().join(format!("gamemeta-scene-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let open = |name: &str, bytes: &[u8]| {
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            let file = File::open(&path).unwrap();
            let size = file.metadata().unwrap().len();
            FileSource { file, size }
        };
        let banner =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/scene/features/alpha-test.cbmd"))
                .unwrap();
        let line = scene_line(&mut open("banner.cbmd", &banner));
        assert!(line.starts_with("{\"animations\":") && line.contains("\"models\":[{"), "{line}");
        assert_eq!(scene_line(&mut open("sample.3ds", &gamemeta_fixtures::sample_3ds())), "null");
        assert_eq!(scene_line(&mut open("empty.bin", &[])), "null");
        for name in ["banner.cbmd", "sample.3ds", "empty.bin"] {
            std::fs::remove_file(dir.join(name)).unwrap();
        }
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn the_info_line_carries_the_version_and_the_typed_cartridge_header() {
        let rom = gamemeta_fixtures::carts::gb::Spec::default().build();
        let info = gamemeta_core::read(&mut gamemeta_core::SliceSource::new(&rom), "game.gb");
        let line = json::info(info.as_ref());
        assert!(line.contains("\"platform\":\"GameBoy\"") && line.contains("\"version\":2,"), "{line}");
        assert!(line.contains("\"cart\":{\"family\":\"gb\",\"title\":\"SYNTH TEST\""), "{line}");
        let plain = json::info(
            gamemeta_core::read(&mut gamemeta_core::SliceSource::new(&gamemeta_fixtures::sample_3ds()), "a.3ds")
                .as_ref(),
        );
        assert!(plain.contains("\"version\":null,\"cart\":null"), "{plain}");
    }

    #[test]
    fn info_is_a_verb_that_reads_like_no_verb() {
        assert_eq!(command_of(&strings(&["info", "a.gb"])), (Some("info".to_string()), strings(&["a.gb"])));
        assert_eq!(command_of(&strings(&["a.gb"])), (None, strings(&["a.gb"])));
        assert_eq!(command_of(&strings(&["ids", "a.gb"])).0.as_deref(), Some("ids"));
    }

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn render_takes_a_file_a_time_a_size_and_an_output() {
        let a = render_args(&strings(&["b.cbmd", "--t", "1.5", "--size", "200x120", "-o", "f.png"])).unwrap();
        assert_eq!(a, RenderArgs { path: "b.cbmd".into(), seconds: 1.5, width: 200, height: 120, out: "f.png".into() });
        let d = render_args(&strings(&["-o", "f.png", "b.cia"])).unwrap();
        assert_eq!((d.seconds, d.width, d.height), (0.0, 400, 240));
        for bad in [
            &["b.cbmd"][..],
            &["-o", "f.png"],
            &["b.cbmd", "--t", "x", "-o", "f.png"],
            &["b.cbmd", "--size", "0x240", "-o", "f.png"],
            &["b.cbmd", "--size", "400", "-o", "f.png"],
            &["b.cbmd", "--size", "9000x9000", "-o", "f.png"],
            &["b.cbmd", "c.cbmd", "-o", "f.png"],
            &["b.cbmd", "--wat", "-o", "f.png"],
        ] {
            assert!(render_args(&strings(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn render_writes_the_reference_renderers_frame_at_that_time() {
        let dir = std::env::temp_dir().join(format!("gamemeta-render-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
        let out = dir.join("frame.png");
        let args = RenderArgs {
            path: format!("{fixtures}/scene/features/bone-anim.cbmd"),
            seconds: 1.0,
            width: 400,
            height: 240,
            out: out.to_str().unwrap().into(),
        };
        render_file(&args).unwrap();
        let ours = gamemeta_core::png::decode(&std::fs::read(&out).unwrap()).unwrap();
        // One second is frame 60: the original implementation's mid-loop golden.
        let golden =
            gamemeta_core::png::decode(&std::fs::read(format!("{fixtures}/render/bone-anim-1.png")).unwrap()).unwrap();
        assert_eq!(ours, golden);
        let sample = dir.join("sample.3ds");
        std::fs::write(&sample, gamemeta_fixtures::sample_3ds()).unwrap();
        let none = RenderArgs { path: sample.to_str().unwrap().into(), ..args };
        assert!(render_file(&none).unwrap_err().contains("no banner"));
        std::fs::remove_file(&out).unwrap();
        std::fs::remove_file(&sample).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn banner3ds_takes_a_file_a_format_a_scale_and_an_output() {
        use banner3ds_export::render::Format;
        let a = export_args(&strings(&["b.cbmd", "--webp", "--scale", "2", "-o", "f.webp"])).unwrap();
        assert_eq!(a, ExportArgs { path: "b.cbmd".into(), format: Format::Webp, scale: 2, out: "f.webp".into() });
        assert_eq!(export_args(&strings(&["--gif", "-o", "f.gif", "b.3ds"])).unwrap().scale, 1);
        for bad in [
            &["b.cbmd", "-o", "f.gif"][..],
            &["b.cbmd", "--gif"],
            &["--gif", "-o", "f.gif"],
            &["b.cbmd", "--gif", "--webp", "-o", "f"],
            &["b.cbmd", "--gif", "--scale", "0", "-o", "f.gif"],
            &["b.cbmd", "--gif", "--scale", "9", "-o", "f.gif"],
            &["b.cbmd", "--gif", "--scale", "x", "-o", "f.gif"],
            &["b.cbmd", "c.cbmd", "--gif", "-o", "f.gif"],
            &["b.cbmd", "--wat", "--gif", "-o", "f.gif"],
        ] {
            assert!(export_args(&strings(bad)).is_err(), "{bad:?}");
        }
        let mp4 = export_args(&strings(&["b.cbmd", "--mp4", "-o", "f.mp4"])).unwrap_err();
        assert!(mp4.contains("website"), "{mp4}");
    }

    #[test]
    fn banner3ds_writes_a_gif_and_a_webp_of_the_animation() {
        use banner3ds_export::render::Format;
        let dir = std::env::temp_dir().join(format!("gamemeta-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/scene/features/bone-anim.cbmd");
        let (gif, webp) = (dir.join("b.gif"), dir.join("b.webp"));
        for (format, out) in [(Format::Gif, &gif), (Format::Webp, &webp)] {
            export_file(&ExportArgs { path: src.into(), format, scale: 1, out: out.to_str().unwrap().into() }).unwrap();
        }
        let (g, w) = (std::fs::read(&gif).unwrap(), std::fs::read(&webp).unwrap());
        assert_eq!(&g[..6], b"GIF89a");
        assert_eq!((&w[..4], &w[8..12]), (&b"RIFF"[..], &b"WEBP"[..]));
        let sample = dir.join("sample.3ds");
        std::fs::write(&sample, gamemeta_fixtures::sample_3ds()).unwrap();
        let none = ExportArgs {
            path: sample.to_str().unwrap().into(),
            format: Format::Gif,
            scale: 1,
            out: gif.to_str().unwrap().into(),
        };
        assert!(export_file(&none).unwrap_err().contains("no banner"));
        assert!(export_file(&ExportArgs { path: dir.join("missing").to_str().unwrap().into(), ..none }).is_err());
        for f in [gif, webp, sample] {
            std::fs::remove_file(f).unwrap();
        }
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn an_rpx_is_read_through_the_folders_beside_its_code_folder() {
        let root = std::env::temp_dir().join(format!("gamemeta-rpx-{}", std::process::id()));
        std::fs::create_dir_all(root.join("code")).unwrap();
        std::fs::create_dir_all(root.join("meta")).unwrap();
        std::fs::write(root.join("code/game.rpx"), [0u8; 16]).unwrap();
        let xml = gamemeta_fixtures::wiiu::Meta::default().xml();
        std::fs::write(root.join("meta/meta.xml"), xml).unwrap();
        let rpx = root.join("code/game.rpx");
        let info = read_rpx(rpx.to_str().unwrap()).expect("title");
        assert_eq!(info.title(), Some("Test Quest"));
        assert!(read_rpx(root.join("nowhere/game.rpx").to_str().unwrap()).is_none());
        std::fs::remove_file(root.join("code/game.rpx")).unwrap();
        std::fs::remove_file(root.join("meta/meta.xml")).unwrap();
        std::fs::remove_dir(root.join("code")).unwrap();
        std::fs::remove_dir(root.join("meta")).unwrap();
        std::fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn ids_come_from_the_file_its_siblings_and_the_ra_hash() {
        let dir = std::env::temp_dir().join(format!("gamemeta-ids-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let line = |name: &str, bytes: &[u8]| {
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            let file = File::open(&path).unwrap();
            let size = file.metadata().unwrap().len();
            ids_line(path.to_str().unwrap(), &mut FileSource { file, size })
        };
        let iso = gamemeta_fixtures::ids::sample_ps2_iso();
        std::fs::write(dir.join("sample.bin"), gamemeta_fixtures::ids::raw2352(&iso, 2)).unwrap();
        let cue = line("sample.cue", gamemeta_fixtures::ids::sample_cue().as_bytes());
        assert!(cue.contains("\"serial\":\"SLUS00003\"") && cue.contains("\"primaryId\":\"SLUS00003\""), "{cue}");
        let rom = line("sample.sfc", &gamemeta_fixtures::ids::sample_rom(4096));
        assert!(rom.contains("\"raMd5\":\"") && !rom.contains("\"raMd5\":null"), "{rom}");
        let zip = line("rom.zip", &gamemeta_fixtures::ids::sample_rom_zip());
        assert!(zip.contains("\"zipSize\":4096"), "{zip}");
        assert_eq!(line("unknown.xyz", b"nothing"), "null");
        for name in ["sample.cue", "sample.bin", "sample.sfc", "rom.zip", "unknown.xyz"] {
            std::fs::remove_file(dir.join(name)).unwrap();
        }
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn wii_banner_takes_a_file_frames_a_scale_and_outputs() {
        let a = wii_banner_args(&strings(&[
            "d.rvz", "--frame", "20", "--poster", "--scale", "0.5", "-o", "f.png", "--bin", "b.bin",
        ]))
        .unwrap();
        assert_eq!(
            a,
            WiiBannerArgs {
                path: "d.rvz".into(),
                frames: vec![Some("20".into()), None],
                scale: 0.5,
                out: Some("f.png".into()),
                bin: Some("b.bin".into()),
                icon: false
            }
        );
        // -o alone is frame 0; --bin alone writes no picture; neither is a summary only.
        assert_eq!(wii_banner_args(&strings(&["b.bin", "-o", "f.png"])).unwrap().frames, vec![Some("0".into())]);
        assert!(wii_banner_args(&strings(&["d.rvz", "--icon"])).unwrap().icon);
        assert!(wii_banner_args(&strings(&["b.bin", "--bin", "x"])).unwrap().out.is_none());
        assert!(wii_banner_args(&strings(&["b.bin"])).is_ok());
        for bad in [
            &[][..],
            &["a", "b"],
            &["a", "--frame"],
            &["a", "--frame", "x", "-o", "f.png"],
            &["a", "--scale", "0", "-o", "f.png"],
            &["a", "--wat"],
            &["a", "--poster"],
        ] {
            assert!(wii_banner_args(&strings(bad)).is_err(), "{bad:?}");
        }
        let f = |s: &str| Some(s.to_string());
        assert_eq!(wii_banner_outputs("a/f.png", &[f("0")]), vec![("a/f.png".into(), f("0"))]);
        assert_eq!(
            wii_banner_outputs("a/f.png", &[f("0"), None]),
            vec![("a/f-0.png".into(), f("0")), ("a/f-poster.png".into(), None)]
        );
        assert_eq!(wii_banner_outputs("a.b/f", &[None, f("1")])[1].0, "a.b/f-1");
    }

    #[test]
    fn wii_banner_writes_the_archive_the_summary_and_the_reference_poster() {
        let dir = std::env::temp_dir().join(format!("gamemeta-wii-banner-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
        let archive = std::fs::read(format!("{fixtures}/wii-banner/banner608.bin")).unwrap();
        let bnr = gamemeta_fixtures::wii::opening(&[("en", "T")], None, Some(&archive), |a| a);
        let input = dir.join("opening.bnr");
        std::fs::write(&input, bnr).unwrap();
        let (png, bin) = (dir.join("p.png"), dir.join("b.bin"));
        let args = WiiBannerArgs {
            path: input.to_str().unwrap().into(),
            frames: vec![None],
            scale: 1.0,
            out: Some(png.to_str().unwrap().into()),
            bin: Some(bin.to_str().unwrap().into()),
            icon: false,
        };
        wii_banner_file(&args).unwrap();
        assert_eq!(std::fs::read(&bin).unwrap(), archive);
        let golden =
            gamemeta_core::png::decode(&std::fs::read(format!("{fixtures}/wii-banner/banner608-poster.png")).unwrap())
                .unwrap();
        assert_eq!(gamemeta_core::png::decode(&std::fs::read(&png).unwrap()).unwrap(), golden);
        let banner = gamemeta_core::wii::banner::WiiBanner::open(&archive).unwrap();
        assert_eq!(
            json::wii_banner(&banner),
            "{\"startFrames\":40,\"loopFrames\":120,\"size\":[608,456],\"skipped\":{\"txt1\":1,\"wnd1\":1},\"missingTextures\":[]}"
        );
        let none = WiiBannerArgs { path: bin.to_str().unwrap().replace("b.bin", "nope"), ..args };
        assert!(wii_banner_file(&none).is_err());
        let junk = dir.join("junk.bin");
        std::fs::write(&junk, [7u8; 50]).unwrap();
        let junk_args = WiiBannerArgs {
            path: junk.to_str().unwrap().into(),
            frames: vec![],
            out: None,
            bin: None,
            scale: 1.0,
            icon: false,
        };
        assert!(wii_banner_file(&junk_args).unwrap_err().contains("no channel banner"));
        for f in [&input, &png, &bin, &junk] {
            std::fs::remove_file(f).unwrap();
        }
        std::fs::remove_dir(&dir).unwrap();
    }
}
