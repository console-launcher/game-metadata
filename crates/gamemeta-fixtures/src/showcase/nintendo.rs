// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The demos for Nintendo's disc, card and download formats that carry art: DS, DSi, 3DS, GameCube, Wii, Wii U.
use super::art::{self, Indexed};
use super::{File, NAME, Options, SUBTITLES, TEAM, le, put, two_lines};
use crate::{gc, nds, wii, wiiu};

pub(crate) fn add(out: &mut Vec<File>, options: &Options) {
    let (nds_rom, dsi_rom) = ds();
    put(out, "tux.nds", nds_rom.clone());
    put(out, "tux.dsi", dsi_rom);
    put(out, "tux.zip", crate::zip::zip(&[("tux.nds", &nds_rom, false)]));
    put(out, "tux-deflate.zip", crate::zip::zip(&[("tux.nds", &nds_rom, true)]));
    let (three_ds, cia) = three_ds(options.banner3ds.as_deref());
    put(out, "tux.3ds", three_ds);
    put(out, "tux.cia", cia);
    gamecube(out);
    wii_files(out);
    wii_u(out);
}

// ------------------------------------------------------------------------------------------------ DS and DSi

fn ds_title(slot: usize) -> String {
    format!("{NAME}\n{}\n{TEAM}", SUBTITLES[slot])
}

fn ds_bitmap(q: &Indexed) -> Vec<u8> {
    nds::bitmap_with(|x, y| q.index(x, y))
}

fn ds_icon(n: usize) -> Indexed {
    art::quantize(&art::icon(n, 32), 16)
}

fn ds_rom(banner: &[u8], unit_code: u8) -> Vec<u8> {
    let mut rom = nds::rom(banner, 0x200, 0);
    rom[..12].copy_from_slice(b"SUPERTUXKART");
    rom[0x0C..0x10].copy_from_slice(b"TUXE");
    rom[0x10..0x12].copy_from_slice(b"00");
    rom[0x12] = unit_code;
    rom
}

/// The DS ROM (banner version 3: eight languages) and the DSi one (0x0103: the animated icon too).
fn ds() -> (Vec<u8>, Vec<u8>) {
    let titles: Vec<String> = (0..8).map(ds_title).collect();
    let icon = ds_icon(0);
    let v3 = nds::banner_with(3, &titles, &ds_bitmap(&icon), &icon.bgr555(), &[], &[]);
    let frames: Vec<(Vec<u8>, Vec<u8>)> = (0..art::FRAMES)
        .map(|n| {
            let q = ds_icon(n);
            (ds_bitmap(&q), q.bgr555())
        })
        .collect();
    // Each step: 6 ticks, bitmap n, palette n.
    let sequence: Vec<u16> = (0..art::FRAMES as u16).map(|n| 6 | (n << 8) | (n << 11)).collect();
    let dsi = nds::banner_with(0x0103, &titles, &ds_bitmap(&icon), &icon.bgr555(), &frames, &sequence);
    (ds_rom(&v3, 0), ds_rom(&dsi, 3))
}

// ------------------------------------------------------------------------------------------------ 3DS

/// The SMDH: 12 titles, publisher, both icons on the sky colour, every region.
fn smdh() -> Vec<u8> {
    let large = art::flatten(&art::icon(0, 48), art::SKY);
    let small = art::flatten(&art::icon(0, 24), art::SKY);
    let names: Vec<String> = (0..12).map(two_lines).collect();
    let slots: Vec<crate::SmdhSlot> =
        (0..12).map(|slot| crate::SmdhSlot { slot, short: NAME, long: &names[slot], publisher: TEAM }).collect();
    crate::smdh_with(
        &slots,
        |i| art::rgb565(art::tiled_pixel(&large, i)),
        |i| art::rgb565(art::tiled_pixel(&small, i)),
        0x7F,
        &[],
    )
}

/// The `.3ds` (NCSD) and `.cia` demos: an unencrypted NCCH whose ExeFS holds the SMDH `icon` and, when given, the `banner`.
fn three_ds(banner: Option<&[u8]>) -> (Vec<u8>, Vec<u8>) {
    let icon = smdh();
    let mut files: Vec<(&str, &[u8])> = Vec::new();
    if let Some(b) = banner {
        files.push(("banner", b));
    }
    files.push(("icon", &icon));
    let ncch = crate::with_product_code(
        crate::with_program_id(crate::plain_ncch(&crate::exefs(&files)), 0x0004_0000_0FF3_FF00),
        "CTR-P-TUXK",
    );
    (crate::ncsd(&ncch), crate::cia(&ncch))
}

// ------------------------------------------------------------------------------------------------ GameCube

fn bnr_texts() -> Vec<gc::Text> {
    // BNR2's six languages, in the disc's order: English, German, French, Spanish, Italian, Dutch.
    [1, 3, 2, 5, 4, 8]
        .into_iter()
        .map(|slot| gc::Text {
            short_title: NAME.into(),
            short_maker: TEAM.into(),
            long_title: NAME.into(),
            long_maker: TEAM.into(),
            description: SUBTITLES[slot].into(),
        })
        .collect()
}

/// The 96x32 banner: the kart turning across the sky.
fn gc_banner_image() -> Vec<u8> {
    let mut sheet = art::gradient(96, 32, art::SKY, art::SKY_DEEP);
    for (i, n) in [0, 2, 4].into_iter().enumerate() {
        sheet = art::paste(&sheet, &art::icon(n, 32), 32 * i as i64, 0);
    }
    gc::image_with(|x, y| art::rgb5a3(sheet_at(&sheet, x, y)))
}

fn sheet_at(image: &gamemeta_core::RgbaImage, x: usize, y: usize) -> [u8; 4] {
    let i = (y * image.width as usize + x) * 4;
    [image.pixels[i], image.pixels[i + 1], image.pixels[i + 2], image.pixels[i + 3]]
}

fn gamecube(out: &mut Vec<File>) {
    let bnr = gc::bnr(&bnr_texts(), &gc_banner_image(), "windows-1252");
    let disc = gc::disc("GTXE00", NAME, &[("opening.bnr", &bnr)], None);
    put(out, "tux-gc.ciso", gc::ciso(&disc, 0x8000));
    put(out, "tux-gc.rvz", gc::rvz(&disc, gc::RvzOptions::default(), b"RVZ\x01"));
    put(out, "tux-gc.iso", disc);
}

// ------------------------------------------------------------------------------------------------ Wii

fn wii_logo() -> Vec<u8> {
    // The reader takes the largest wide texture with transparency as the logo: Tux on nothing.
    let mut sheet = art::from_fn(128, 64, |_, _| [0; 4]);
    sheet = art::paste(&sheet, &art::icon(0, 64), 0, 0);
    sheet = art::paste(&sheet, &art::icon(3, 64), 64, 0);
    let tpl = wii::tpl_plain(128, 64, 6, &wii::rgba8(128, 64, |x, y| art::argb(sheet_at(&sheet, x, y))));
    wii::bin_archive(&tpl, "Logo.tpl", true, false, &[])
}

/// The channel banner (`meta/banner.bin`): a 608x456 layout, a sky gradient behind the kart, which slides left and right
/// forever (a looping `RLPA` animation of the pane's x). Built with the same writer the banner renderer is tested against.
fn wii_banner() -> Vec<u8> {
    use crate::wii_banner::{self as wb, Key, Mat, Pane, Tag, Target, Tex};
    let kart = art::icon(0, 128);
    let texture = wb::tpl_words(128, 128, |x, y| art::rgb5a3(sheet_at(&kart, x, y)));
    let sky = |c: [u8; 3]| u32::from_be_bytes([c[0], c[1], c[2], 255]);
    let layout = wb::layout(
        608.0,
        456.0,
        &["tux.tpl"],
        &[Mat::new("sky"), Mat { textures: vec![Tex(0, 0)], ..Mat::new("kart") }],
        &[
            Pane {
                name: "sky",
                w: 608.0,
                h: 456.0,
                colors: [sky(art::SKY), sky(art::SKY), sky(art::SKY_DEEP), sky(art::SKY_DEEP)],
                ..Pane::new("sky")
            },
            Pane { name: "kart", w: 256.0, h: 256.0, material: 1, ..Pane::new("kart") },
        ],
    );
    let slide = wb::animation(
        120,
        true,
        &[Target {
            name: "kart",
            material: false,
            tags: vec![Tag(
                "RLPA",
                vec![wb::track(0, 0, true, &[Key(0.0, -150.0, 0.0), Key(60.0, 150.0, 0.0), Key(120.0, -150.0, 0.0)])],
            )],
        }],
        &[],
    );
    wb::archive(&layout, &[("banner_Loop.brlan", &slide)], &[("tux.tpl", &texture)])
}

fn wii_files(out: &mut Vec<File>) {
    // The IMET title slots: ja, en, de, fr, es, it, nl, zhs, zht, ko (the 3DS slots of the same languages).
    let slots = [
        ("ja", 0),
        ("en", 1),
        ("de", 3),
        ("fr", 2),
        ("es", 5),
        ("it", 4),
        ("nl", 8),
        ("zhs", 6),
        ("zht", 11),
        ("ko", 7),
    ];
    let titles: Vec<(&str, String)> = slots.iter().map(|(code, slot)| (*code, two_lines(*slot))).collect();
    let titles: Vec<(&str, &str)> = titles.iter().map(|(c, t)| (*c, t.as_str())).collect();
    let bnr = wii::opening_plain(&titles, Some(&wii_logo()), Some(&wii_banner()));
    let part = wii::partition_data("RTXE00", NAME, &[wii::PartFile("opening.bnr", &bnr)], None, 0x7C00);
    let rvz = wii::rvz(&part, &wii::WiiRvzOptions { game_id: "RTXE00", title: NAME, ..wii::WiiRvzOptions::default() });
    put(out, "tux-wii.rvz", rvz.bytes);
    put(out, "tux-wii.iso", wii::disc_header("RTXE00", NAME, 0x440));
    put(out, "tux-wii.wbfs", wii::wbfs("RTXE00", NAME));
    put(out, "tux.wad", wii::wad(0x0001_0001_5455_584B, 0x0100, "Is"));
}

// ------------------------------------------------------------------------------------------------ Wii U

fn splash(width: u32, height: u32, kart: u32, frame: usize) -> gamemeta_core::RgbaImage {
    let bg = art::gradient(width, height, art::SKY, art::SKY_DEEP);
    art::paste(&bg, &art::icon(frame, kart), ((width - kart) / 2) as i64, ((height - kart) / 2) as i64)
}

fn tga_of(image: &gamemeta_core::RgbaImage, bpp: usize) -> Vec<u8> {
    wiiu::tga(image.width as usize, image.height as usize, bpp, true, true, |x, y| art::bgra(sheet_at(image, x, y)))
}

fn wii_u(out: &mut Vec<File>) {
    let langs = wiiu::LANGS;
    let meta = wiiu::Meta {
        longs: (0..12).map(|s| (langs[s], two_lines(s))).collect(),
        shorts: (0..12).map(|s| (langs[s], NAME.to_string())).collect(),
        publishers: (0..12).map(|s| (langs[s], TEAM.to_string())).collect(),
        title_id: "000500001FF3FF00",
        product_code: "WUP-P-TUXE",
        region: "00000007",
    };
    let files: Vec<(String, Vec<u8>)> = vec![
        ("meta/meta.xml".into(), meta.xml().into_bytes()),
        ("meta/iconTex.tga".into(), tga_of(&art::icon(0, 128), 32)),
        ("meta/bootTvTex.tga".into(), tga_of(&splash(1280, 720, 400, 1), 24)),
        ("meta/bootDrcTex.tga".into(), tga_of(&splash(854, 480, 300, 2), 24)),
        // A stub: the ELF magic and nothing else (the demo carries no game code).
        ("code/tux.rpx".into(), {
            let mut b = le(16);
            b.bytes(0, b"\x7FELF");
            b.0
        }),
    ];
    for (path, bytes) in &files {
        put(out, &format!("tux-wiiu/{path}"), bytes.clone());
    }
    let in_archive: Vec<(String, Vec<u8>)> =
        files.into_iter().map(|(p, d)| (format!("000500001ff3ff00_v0/{p}"), d)).collect();
    put(out, "tux.wua", wiiu::zarchive(&in_archive));
}
