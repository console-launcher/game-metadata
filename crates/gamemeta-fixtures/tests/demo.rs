// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The SuperTuxKart demo set read by the library: per format, what the readers must give back. Written before the
//! generator (red first), then the generator was made to satisfy it.
use gamemeta_core::carts::Field;
use gamemeta_core::{GameInfo, ImageKind, Language, Platform};
use gamemeta_fixtures::showcase::{self, hash, report};
use std::sync::OnceLock;

fn files() -> &'static Vec<(String, Vec<u8>)> {
    static FILES: OnceLock<Vec<(String, Vec<u8>)>> = OnceLock::new();
    FILES.get_or_init(showcase::files)
}

fn bytes(name: &str) -> &'static [u8] {
    &files().iter().find(|(p, _)| p == name).unwrap_or_else(|| panic!("{name} is not in the demo set")).1
}

fn info(name: &str) -> GameInfo {
    report::read(files(), name).info.unwrap_or_else(|| panic!("{name}: the metadata reader gave nothing"))
}

fn ids(name: &str) -> gamemeta_core::ids::Ids {
    report::read(files(), name).ids.unwrap_or_else(|| panic!("{name}: the ids reader gave nothing"))
}

fn size(image: &Option<gamemeta_core::RgbaImage>) -> Option<(u32, u32)> {
    image.as_ref().map(|i| (i.width, i.height))
}

fn name_in(i: &GameInfo, l: Language) -> String {
    i.title_in(l).unwrap_or_else(|| panic!("no {l:?} title")).name.clone()
}

fn publisher_in(i: &GameInfo, l: Language) -> Option<String> {
    i.title_in(l).and_then(|t| t.publisher.clone())
}

const TEAM: &str = "SuperTuxKart Team";

#[test]
fn the_ds_rom_gives_eight_titles_a_32px_icon_and_homebrew_ids() {
    let i = info("tux.nds");
    assert_eq!(i.platform, Platform::Ds);
    let langs: Vec<Language> = i.titles.iter().map(|(l, _)| *l).collect();
    use Language::*;
    assert_eq!(langs, [Japanese, English, French, German, Italian, Spanish, ChineseSimplified, Korean]);
    assert!(i.titles.iter().all(|(_, t)| t.name.starts_with("SuperTuxKart")), "{:?}", i.titles);
    assert_eq!(name_in(&i, English), "SuperTuxKart Kart racing");
    assert_eq!(publisher_in(&i, English).as_deref(), Some(TEAM));
    assert_eq!(size(&i.icon), Some((32, 32)));
    assert!(i.animated_icon.is_empty());
    assert_eq!(i.ids.game_id.as_deref(), Some("TUXE"));
    assert_eq!(i.ids.maker_code.as_deref(), Some("00"));
}

#[test]
fn the_dsi_rom_gives_an_eight_frame_animated_icon_and_the_dsi_banner_block() {
    let i = info("tux.dsi");
    assert_eq!(i.platform, Platform::Dsi);
    assert_eq!(size(&i.icon), Some((32, 32)));
    assert_eq!(i.animated_icon.len(), 8);
    assert!(i.animated_icon.iter().all(|s| s.duration_frames == 6 && (s.frame.width, s.frame.height) == (32, 32)));
    let first = &i.animated_icon[0].frame;
    assert!(i.animated_icon.iter().skip(1).all(|s| s.frame != *first), "the frames differ");
    assert_eq!(i.banner.map(|b| b.size), Some(0x23C0));
    assert_eq!(i.titles.len(), 8);
}

#[test]
fn the_three_ds_files_give_twelve_titles_both_icons_and_no_banner_yet() {
    for name in ["tux.3ds", "tux.cia"] {
        let i = info(name);
        assert_eq!(i.platform, Platform::ThreeDs, "{name}");
        assert_eq!(i.titles.len(), 12, "{name}");
        assert_eq!(name_in(&i, Language::English), "SuperTuxKart Kart racing", "{name}");
        assert_eq!(publisher_in(&i, Language::Korean).as_deref(), Some(TEAM), "{name}");
        assert_eq!(size(&i.icon), Some((48, 48)), "{name}");
        assert_eq!(size(&i.small_icon), Some((24, 24)), "{name}");
        assert_eq!(i.ids.title_id.as_deref(), Some("000400000FF3FF00"), "{name}");
        assert_eq!(i.ids.product_code.as_deref(), Some("CTR-P-TUXK"), "{name}");
        assert!(i.banner.is_some(), "{name}: the showcase banner");
    }
}

#[test]
fn the_three_ds_banner_hook_adds_a_banner_to_both_files() {
    let banner = gamemeta_fixtures::cbmd(&[0xC6; 32], &[]);
    let with = showcase::build(&showcase::Options { banner3ds: Some(banner) });
    for name in ["tux.3ds", "tux.cia"] {
        let bytes = &with.iter().find(|(p, _)| p == name).unwrap().1;
        let i = gamemeta_core::read(&mut gamemeta_core::SliceSource::new(bytes), name).expect(name);
        assert!(i.banner.is_some(), "{name}");
    }
}

#[test]
fn the_gamecube_disc_and_its_ciso_and_rvz_give_a_bnr2_with_a_96x32_banner() {
    for name in ["tux-gc.iso", "tux-gc.ciso", "tux-gc.rvz"] {
        let i = info(name);
        assert_eq!(i.platform, Platform::GameCube, "{name}");
        assert_eq!(size(&i.icon), Some((96, 32)), "{name}");
        assert_eq!(i.titles.len(), 6, "{name}");
        assert_eq!(name_in(&i, Language::German), "SuperTuxKart", "{name}");
        assert_eq!(i.ids.game_id.as_deref(), Some("GTXE00"), "{name}");
        assert_eq!(i.header_title.as_deref(), Some("SuperTuxKart"), "{name}");
    }
}

#[test]
fn the_wii_rvz_gives_titles_and_the_logo_and_the_other_wii_files_give_their_headers() {
    let i = info("tux-wii.rvz");
    assert_eq!(i.platform, Platform::Wii);
    assert_eq!(i.ids.game_id.as_deref(), Some("RTXE00"));
    assert_eq!(i.titles.len(), 10);
    assert_eq!(name_in(&i, Language::French), "SuperTuxKart Course de karts");
    assert!(i.images.contains(&ImageKind::Logo), "{:?}", i.images);
    assert_eq!(size(&i.icon), Some((128, 64)), "the logo");
    for name in ["tux-wii.iso", "tux-wii.wbfs"] {
        let i = info(name);
        assert_eq!(i.platform, Platform::Wii, "{name}");
        assert_eq!(i.ids.game_id.as_deref(), Some("RTXE00"), "{name}");
        assert_eq!(i.header_title.as_deref(), Some("SuperTuxKart"), "{name}");
    }
}

#[test]
fn the_wad_gives_its_title_id() {
    let ids = ids("tux.wad");
    assert_eq!(ids.title_id.as_deref(), Some("000100015455584B"));
    assert_eq!(info("tux.wad").ids.game_id.as_deref(), Some("TUXK"));
}

#[test]
fn the_wii_u_title_folder_and_the_wua_give_twelve_titles_and_the_images() {
    for name in ["tux-wiiu/code/tux.rpx", "tux.wua"] {
        let i = info(name);
        assert_eq!(i.platform, Platform::WiiU, "{name}");
        assert_eq!(i.titles.len(), 12, "{name}");
        assert_eq!(name_in(&i, Language::Japanese), "SuperTuxKart カートレース", "{name}");
        assert_eq!(size(&i.icon), Some((128, 128)), "{name}");
        assert_eq!(i.ids.title_id.as_deref(), Some("000500001FF3FF00"), "{name}");
        assert_eq!(i.ids.product_code.as_deref(), Some("WUP-P-TUXE"), "{name}");
        for kind in [ImageKind::Icon, ImageKind::TvSplash, ImageKind::GamePadSplash] {
            assert!(i.images.contains(&kind), "{name}: {kind:?}");
        }
    }
}

#[test]
fn the_psp_files_give_the_title_and_the_disc_id() {
    for name in ["tux.pbp", "tux-psp.iso"] {
        let ids = ids(name);
        assert_eq!(ids.psp_title.as_deref(), Some("SuperTuxKart"), "{name}");
        assert_eq!(ids.psp_disc_id.as_deref(), Some("ULUS99901"), "{name}");
    }
}

#[test]
fn the_playstation_discs_give_their_serials_and_the_bin_follows_its_cue() {
    assert_eq!(ids("tux-ps1.cue").serial.as_deref(), Some("SLUS99901"));
    assert_eq!(ids("tux-ps1.bin").serial.as_deref(), Some("SLUS99901"));
    assert_eq!(ids("tux-ps2.iso").serial.as_deref(), Some("SLUS99902"));
    assert_eq!(bytes("tux-ps1.bin").len() % 2352, 0);
}

#[test]
fn the_switch_xbox_chd_and_zip_demos_give_their_ids() {
    assert_eq!(ids("tux.nsp").title_id.as_deref(), Some("0100F3FF0000A000"));
    assert_eq!(ids("tux.xbe").xbox_id.as_deref(), Some("FFFE0001"));
    let iso = bytes("tux-ps2.iso");
    assert_eq!(ids("tux.chd").chd_sha1.as_deref(), Some(hash::hex(&hash::sha1(iso)).as_str()));
    let rom = bytes("tux.nds");
    for name in ["tux.zip", "tux-deflate.zip"] {
        let z = ids(name);
        assert_eq!(z.zip_crc.as_deref(), Some(gamemeta_fixtures::ids::crc_hex(rom).as_str()), "{name}");
        assert_eq!(z.zip_size, Some(rom.len() as u64), "{name}");
        let i = info(name);
        assert_eq!(i.platform, Platform::Ds, "{name}: the zip's ROM is read");
        assert_eq!(size(&i.icon), Some((32, 32)), "{name}");
    }
    assert!(bytes("tux-deflate.zip").len() < bytes("tux.zip").len(), "the second zip is deflated");
}

fn cart(name: &str, field: &str) -> Field {
    let i = info(name);
    let header = i.cart.unwrap_or_else(|| panic!("{name}: no cartridge header"));
    header.fields().into_iter().find(|(k, _)| *k == field).unwrap_or_else(|| panic!("{name}: no field {field}")).1
}

fn text(s: &str) -> Field {
    Field::Text(s.into())
}

#[test]
fn the_cartridge_demos_are_valid_headers_titled_supertuxkart() {
    for (name, platform) in [
        ("tux.gb", Platform::GameBoy),
        ("tux.gbc", Platform::GameBoyColor),
        ("tux.gba", Platform::GameBoyAdvance),
        ("tux.z64", Platform::Nintendo64),
        ("tux.v64", Platform::Nintendo64),
        ("tux.n64", Platform::Nintendo64),
        ("tux.sfc", Platform::Snes),
        ("tux.nes", Platform::Nes),
        ("tux.md", Platform::MegaDrive),
        ("tux.sms", Platform::MasterSystem),
        ("tux.gg", Platform::GameGear),
    ] {
        assert_eq!(info(name).platform, platform, "{name}");
    }
    for name in ["tux.gb", "tux.gbc", "tux.gba", "tux.z64", "tux.v64", "tux.n64", "tux.sfc"] {
        assert_eq!(cart(name, "title"), text("SUPERTUXKART"), "{name}");
    }
    assert_eq!(cart("tux.md", "overseas_title"), text("SUPERTUXKART"));
    assert_eq!(cart("tux.gb", "header_checksum_valid"), Field::Flag(true));
    assert_eq!(cart("tux.gb", "logo_valid"), Field::Flag(true));
    assert_eq!(cart("tux.gba", "complement_valid"), Field::Flag(true));
    assert_eq!(cart("tux.sfc", "checksum_valid"), Field::Flag(true));
    assert_eq!(cart("tux.gg", "game_gear"), Field::Flag(true));
    assert_eq!(cart("tux.nes", "mapper"), Field::Number(0));
    for name in ["tux.gb", "tux.gbc", "tux.md", "tux.sms", "tux.gg"] {
        assert_eq!(report::read(files(), name).checksum, Some(true), "{name}: the whole-file checksum matches");
    }
}

#[test]
fn the_n64_byte_orders_all_read_as_the_same_game() {
    let z = info("tux.z64");
    for name in ["tux.v64", "tux.n64"] {
        assert_eq!(
            info(name).cart.map(|c| c.fields().into_iter().filter(|(k, _)| *k != "byte_order").collect::<Vec<_>>()),
            z.cart.clone().map(|c| c.fields().into_iter().filter(|(k, _)| *k != "byte_order").collect::<Vec<_>>()),
            "{name}"
        );
    }
}

#[test]
fn the_set_is_small_and_every_file_is_named_in_the_readme_table() {
    let total: usize = files().iter().map(|(_, b)| b.len()).sum();
    assert!(total < 3 * 1024 * 1024, "the demo set is {total} bytes");
    assert!(files().iter().any(|(p, _)| p == "tux.nds"), "the set is not empty");
}

fn rgba_of_565(p: [u8; 4]) -> [u8; 4] {
    let v = u32::from(showcase::art::rgb565(p));
    // The readers spread the 5 and 6 bit channels by bit replication.
    let (r, g, b) = (v >> 11, (v >> 5) & 63, v & 31);
    [((r << 3) | (r >> 2)) as u8, ((g << 2) | (g >> 4)) as u8, ((b << 3) | (b >> 2)) as u8, 255]
}

#[test]
fn the_icons_the_readers_extract_are_the_prepared_art_pixel_for_pixel() {
    use showcase::art;
    // DS: the quantized 16 colour icon; the DSi's frames likewise.
    let ds = info("tux.nds").icon.unwrap();
    let want = art::expand(&art::quantize(&art::icon(0, 32), 16));
    if let Some(i) = (0..ds.pixels.len()).find(|&i| ds.pixels[i] != want.pixels[i]) {
        panic!(
            "first difference at pixel {} channel {}: {:?} vs {:?}",
            i / 4,
            i % 4,
            &ds.pixels[i / 4 * 4..i / 4 * 4 + 4],
            &want.pixels[i / 4 * 4..i / 4 * 4 + 4]
        );
    }
    let dsi = info("tux.dsi");
    for (n, step) in dsi.animated_icon.iter().enumerate() {
        assert_eq!(step.frame, art::expand(&art::quantize(&art::icon(n, 32), 16)), "frame {n}");
    }
    // 3DS: RGB565 on the sky colour, through the Morton tile order, both sizes.
    for (side, got) in [(48, info("tux.3ds").icon.unwrap()), (24, info("tux.3ds").small_icon.unwrap())] {
        let want = art::flatten(&art::icon(0, side), art::SKY);
        let want: Vec<u8> = want.pixels.chunks(4).flat_map(|p| rgba_of_565([p[0], p[1], p[2], p[3]])).collect();
        if let Some(i) = (0..want.len()).find(|&i| got.pixels[i] != want[i]) {
            panic!(
                "3DS {side}px icon: first difference at pixel {} ({},{}) channel {}: {:?} vs {:?}",
                i / 4,
                i / 4 % side as usize,
                i / 4 / side as usize,
                i % 4,
                &got.pixels[i / 4 * 4..i / 4 * 4 + 4],
                &want[i / 4 * 4..i / 4 * 4 + 4]
            );
        }
    }
    // Wii U: a TGA, lossless.
    assert_eq!(info("tux.wua").icon.unwrap(), art::icon(0, 128));
}

#[test]
fn the_art_preparation_is_deterministic_and_the_quantizer_keeps_transparency() {
    use showcase::art;
    assert_eq!(art::icon(3, 32), art::icon(3, 32));
    let q = art::quantize(&art::icon(0, 32), 16);
    assert!(q.palette.len() <= 16);
    assert_eq!(q.index(0, 0), 0, "the corner is empty, so index 0");
    assert!((0..32).any(|y| (0..32).any(|x| q.index(x, y) != 0)));
    let (opaque, clear) = q.indices.iter().fold((0, 0), |(o, c), &i| if i == 0 { (o, c + 1) } else { (o + 1, c) });
    assert!(opaque > 100 && clear > 100, "{opaque} opaque, {clear} clear");
}

#[test]
fn the_generator_is_deterministic() {
    assert!(showcase::files() == showcase::files());
}

#[test]
fn the_wii_disc_carries_an_animated_channel_banner_that_renders_and_moves() {
    let mut source = gamemeta_core::SliceSource::new(bytes("tux-wii.rvz"));
    let banner =
        gamemeta_core::wii::open_channel_banner(&mut source).unwrap().expect("the opening.bnr has a banner.bin");
    assert!(banner.missing_textures().is_empty(), "{:?}", banner.missing_textures());
    assert!(banner.loop_frames() > 0, "an animation that loops");
    let (a, b) = (banner.render(0.0, 0.5).expect("frame 0"), banner.render(30.0, 0.5).expect("frame 30"));
    assert!(a.pixels.chunks(4).any(|p| p[3] > 0));
    assert!(a != b, "the kart moves");
}
