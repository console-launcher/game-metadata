// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! PSP art (ICON0, PIC1, ICON1.PMF) and the PS3 and Vita `PARAM.SFO` readers, on synthesized files.

use std::collections::HashMap;

use gamemeta_core::sony::{self, MovieKind};
use gamemeta_core::wiiu::TitleFiles;
use gamemeta_core::{Format, ImageKind, Language, Platform, Region, SliceSource, format_for, png};
use gamemeta_fixtures::ids as fx;

const PALETTE: [[u8; 3]; 2] = [[200, 10, 20], [1, 2, 3]];

fn icon0() -> Vec<u8> {
    fx::png_indexed(2, 2, &PALETTE, &[0, 1, 1, 0])
}

fn pic1() -> Vec<u8> {
    let image = gamemeta_core::RgbaImage { width: 3, height: 1, pixels: vec![9, 8, 7, 255, 1, 1, 1, 255, 0, 0, 0, 0] };
    png::encode(&image).unwrap()
}

const PMF: &[u8] = b"PSMF0012 and then the movie";

fn psp_sfo() -> Vec<u8> {
    fx::sfo(&[("DISC_ID", "ULUS10041"), ("TITLE", "Daxter")])
}

fn psp_iso() -> Vec<u8> {
    fx::iso9660(&[
        ("PSP_GAME/PARAM.SFO;1", &psp_sfo()),
        ("PSP_GAME/ICON0.PNG;1", &icon0()),
        ("PSP_GAME/ICON1.PMF;1", PMF),
        ("PSP_GAME/PIC1.PNG;1", &pic1()),
    ])
}

fn psp_pbp() -> Vec<u8> {
    fx::pbp_with(&psp_sfo(), [&icon0(), PMF, &[], &pic1(), &[]])
}

#[test]
fn an_indexed_png_decodes() {
    let image = png::decode(&icon0()).expect("colour type 3");
    assert_eq!((image.width, image.height), (2, 2));
    assert_eq!(&image.pixels[..4], &[200, 10, 20, 255]);
    assert_eq!(&image.pixels[4..8], &[1, 2, 3, 255]);
}

#[test]
fn a_psp_iso_and_an_eboot_give_title_id_icon_background_and_movie() {
    for (name, bytes) in [("g.iso", psp_iso()), ("EBOOT.PBP", psp_pbp())] {
        let mut s = SliceSource::new(&bytes);
        let info = sony::read_psp_info(&mut s, name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(info.platform, Platform::Psp, "{name}");
        assert_eq!(info.title(), Some("Daxter"), "{name}");
        assert_eq!(info.ids.product_code.as_deref(), Some("ULUS10041"), "{name}");
        assert_eq!(info.regions, [Region::NorthAmerica], "{name}");
        assert_eq!(info.icon.as_ref().map(|i| (i.width, i.height)), Some((2, 2)), "{name}");
        assert_eq!(info.images, [ImageKind::Icon, ImageKind::Background], "{name}");
        assert_eq!(info.boot_movie, Some(PMF.len() as u64), "{name}");
        let background = sony::psp_image(&mut s, name, ImageKind::Background).unwrap();
        assert_eq!((background.width, background.height), (3, 1), "{name}");
        assert_eq!(&background.pixels[..4], &[9, 8, 7, 255], "{name}");
        assert_eq!(sony::psp_image(&mut s, name, ImageKind::Icon).unwrap().width, 2, "{name}");
        assert!(sony::psp_image(&mut s, name, ImageKind::Banner).is_none(), "{name}");
        let movie = sony::psp_movie(&mut s, name).unwrap();
        assert_eq!((movie.kind, movie.bytes.as_slice()), (MovieKind::Pmf, PMF), "{name}");
    }
}

#[test]
fn the_generic_entry_reads_a_psp_iso_and_an_eboot() {
    assert_eq!(format_for("EBOOT.PBP"), Some(Format::Pbp));
    for (name, bytes) in [("g.iso", psp_iso()), ("EBOOT.PBP", psp_pbp())] {
        let info = gamemeta_core::read(&mut SliceSource::new(&bytes), name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!((info.platform, info.title()), (Platform::Psp, Some("Daxter")), "{name}");
    }
}

#[test]
fn a_psp_game_without_art_or_with_damaged_art_still_gives_its_title() {
    let bare = fx::iso9660(&[("PSP_GAME/PARAM.SFO;1", &psp_sfo())]);
    let info = sony::read_psp_info(&mut SliceSource::new(&bare), "g.iso").unwrap();
    assert_eq!(
        (info.title(), info.icon.is_none(), info.images.is_empty(), info.boot_movie),
        (Some("Daxter"), true, true, None)
    );
    let bad = fx::iso9660(&[
        ("PSP_GAME/PARAM.SFO;1", &psp_sfo()),
        ("PSP_GAME/ICON0.PNG;1", b"not a png"),
        ("PSP_GAME/ICON1.PMF;1", b"not a movie"),
    ]);
    let mut s = SliceSource::new(&bad);
    let info = sony::read_psp_info(&mut s, "g.iso").unwrap();
    assert!(info.icon.is_none() && info.images.is_empty() && info.boot_movie.is_none());
    assert!(sony::psp_movie(&mut s, "g.iso").is_none());
    let pbp = fx::pbp_with(&psp_sfo(), [&[], &[], &[], &[], &[]]);
    assert!(sony::read_psp_info(&mut SliceSource::new(&pbp), "a.pbp").unwrap().images.is_empty());
}

#[test]
fn files_that_are_not_psp_games_give_nothing() {
    assert!(sony::read_psp_info(&mut SliceSource::new(&[0u8; 4096]), "a.iso").is_none());
    assert!(sony::read_psp_info(&mut SliceSource::new(&fx::sample_ps2_iso()), "a.iso").is_none());
    assert!(sony::read_psp_info(&mut SliceSource::new(&psp_pbp()), "a.bin").is_none());
    assert!(sony::psp_image(&mut SliceSource::new(&[]), "a.pbp", ImageKind::Icon).is_none());
    assert!(sony::psp_movie(&mut SliceSource::new(&[0u8; 8]), "a.pbp").is_none());
}

// ---- PS3 and Vita

struct Files(HashMap<&'static str, Vec<u8>>);

impl TitleFiles for Files {
    fn size(&mut self, path: &str) -> Option<u64> {
        self.0.get(path).map(|b| b.len() as u64)
    }
    fn read(&mut self, path: &str, max: usize) -> Option<Vec<u8>> {
        self.0.get(path).filter(|b| b.len() <= max).cloned()
    }
}

fn files(list: &[(&'static str, Vec<u8>)]) -> Files {
    Files(list.iter().cloned().collect())
}

fn ps3_sfo(id: &str) -> Vec<u8> {
    fx::sfo(&[("TITLE", "Heavy Fire"), ("TITLE_01", "Heavy Fire US"), ("TITLE_04", "Schweres Feuer"), ("TITLE_ID", id)])
}

#[test]
fn a_ps3_disc_folder_gives_title_id_region_languages_and_icon() {
    let mut f = files(&[("PS3_GAME/PARAM.SFO", ps3_sfo("BLUS30001")), ("PS3_GAME/ICON0.PNG", icon0())]);
    let info = sony::read_ps3_title(&mut f).unwrap();
    assert_eq!(info.platform, Platform::Ps3);
    assert_eq!(info.ids.title_id.as_deref(), Some("BLUS30001"));
    assert_eq!(info.regions, [Region::NorthAmerica]);
    assert_eq!(info.title_in(Language::English).map(|t| t.name.as_str()), Some("Heavy Fire US"));
    assert_eq!(info.title_in(Language::German).map(|t| t.name.as_str()), Some("Schweres Feuer"));
    assert_eq!(info.icon.as_ref().map(|i| i.width), Some(2));
    assert_eq!(info.images, [ImageKind::Icon]);
}

#[test]
fn a_ps3_game_folder_and_the_other_regions_read_too() {
    for (id, region) in [("BLES00001", Region::Europe), ("BLJM60001", Region::Japan), ("BLKS20001", Region::Korea)] {
        let mut f = files(&[("PARAM.SFO", fx::sfo(&[("TITLE", "Heavy Fire"), ("TITLE_ID", id)]))]);
        let info = sony::read_ps3_title(&mut f).unwrap();
        assert_eq!((info.regions.as_slice(), info.icon.is_none()), ([region].as_slice(), true), "{id}");
        assert_eq!(info.title(), Some("Heavy Fire"), "{id}: TITLE is the English name when TITLE_01 is absent");
    }
    assert!(sony::read_ps3_title(&mut files(&[])).is_none());
    assert!(sony::read_ps3_title(&mut files(&[("PS3_GAME/PARAM.SFO", b"junk".to_vec())])).is_none());
}

fn vita_sfo(id: &str) -> Vec<u8> {
    fx::sfo(&[("STITLE", "Gravity Rush"), ("TITLE", "Gravity\nRush Remastered"), ("TITLE_ID", id)])
}

#[test]
fn a_vita_folder_gives_title_short_title_id_region_and_icon() {
    let mut f = files(&[("sce_sys/param.sfo", vita_sfo("PCSE00001")), ("sce_sys/icon0.png", icon0())]);
    let info = sony::read_vita_title(&mut f).unwrap();
    assert_eq!(info.platform, Platform::Vita);
    assert_eq!(info.ids.title_id.as_deref(), Some("PCSE00001"));
    assert_eq!(info.title(), Some("Gravity Rush Remastered"));
    assert_eq!(info.preferred_title().map(|(_, t)| t.short_name.as_str()), Some("Gravity Rush"));
    assert_eq!(info.regions, [Region::NorthAmerica]);
    assert_eq!(info.icon.as_ref().map(|i| i.width), Some(2));
    for (id, region) in [("PCSB00001", Region::Europe), ("PCSG00001", Region::Japan), ("PCSC00001", Region::Japan)] {
        let mut f = files(&[("sce_sys/param.sfo", vita_sfo(id))]);
        assert_eq!(sony::read_vita_title(&mut f).unwrap().regions, [region], "{id}");
    }
    assert!(sony::read_vita_title(&mut files(&[("sce_sys/param.sfo", b"junk".to_vec())])).is_none());
}

#[test]
fn a_vpk_gives_the_same_as_the_folder() {
    let zip = gamemeta_fixtures::zip::zip(&[
        ("eboot.bin", b"code".as_slice(), false),
        ("sce_sys/param.sfo", &vita_sfo("PCSB00002"), true),
        ("sce_sys/icon0.png", &icon0(), false),
    ]);
    assert_eq!(format_for("Game.VPK"), Some(Format::Vpk));
    let info = gamemeta_core::read(&mut SliceSource::new(&zip), "Game.VPK").unwrap();
    assert_eq!(
        (info.platform, info.title(), info.ids.title_id.as_deref()),
        (Platform::Vita, Some("Gravity Rush Remastered"), Some("PCSB00002"))
    );
    assert_eq!(info.regions, [Region::Europe]);
    assert_eq!(info.icon.as_ref().map(|i| i.width), Some(2));
    let none = gamemeta_fixtures::zip::zip(&[("eboot.bin", b"code".as_slice(), false)]);
    assert!(sony::read_vpk_info(&mut SliceSource::new(&none)).is_none());
    assert!(sony::read_vpk_info(&mut SliceSource::new(b"not a zip")).is_none());
}

#[test]
fn the_vita_vpk_is_a_listed_format() {
    let rows = gamemeta_core::supported_formats();
    assert!(rows.iter().any(|r| r.extensions.contains(&"vpk")));
}
