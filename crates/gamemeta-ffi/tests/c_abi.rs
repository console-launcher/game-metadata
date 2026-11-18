// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The C ABI through its extern functions, as a C caller would use them.

use std::ffi::{CStr, c_char, c_void};
use std::ptr::{null, null_mut};

use gamemeta::*;

const NAME: *const c_char = c"sample.3ds".as_ptr();

fn read_bytes(bytes: &[u8], name: *const c_char) -> (GmStatus, *mut GmInfo) {
    let mut out = null_mut();
    let status = unsafe { gm_read_bytes(bytes.as_ptr(), bytes.len(), name, &mut out) };
    (status, out)
}

fn text(p: *const c_char) -> Option<String> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_owned())
}

#[test]
fn reads_the_sample_through_every_accessor() {
    let (status, info) = read_bytes(&gamemeta_fixtures::sample_3ds(), NAME);
    assert_eq!(status, GmStatus::Ok);
    unsafe {
        assert_eq!(gm_info_platform(info), GmPlatform::ThreeDs as u32);
        assert_eq!(text(gm_info_title(info, 1, GmTitleField::Name as u32)).as_deref(), Some("Sample Game Subtitle"));
        assert_eq!(text(gm_info_title(info, 1, GmTitleField::Publisher as u32)).as_deref(), Some("Example Pub"));
        assert_eq!(text(gm_info_title(info, 2, GmTitleField::Publisher as u32)), None);
        assert_eq!(text(gm_info_title(info, 99, GmTitleField::Name as u32)), None);
        assert_eq!(gm_info_preferred_language(info), 1);
        assert_eq!(text(gm_info_id(info, GmId::TitleId as u32)).as_deref(), Some("0004000000123400"));
        assert_eq!(text(gm_info_id(info, GmId::ProductCode as u32)).as_deref(), Some("CTR-P-SMPL"));
        let (mut w, mut h) = (0, 0);
        assert!(!gm_info_icon(info, GmIcon::Large as u32, &mut w, &mut h).is_null());
        assert_eq!((w, h), (48, 48));
        assert!(!gm_info_icon(info, GmIcon::Small as u32, &mut w, null_mut()).is_null());
        assert_eq!(w, 24);
        assert_eq!(gm_info_regions(info), 0b110);
        assert!(!gm_info_region_free(info));
        assert_eq!(gm_info_rating(info, 1), 10);
        assert_eq!(gm_info_rating(info, 3), -1);
        let (mut off, mut size) = (0, 0);
        assert!(gm_info_banner(info, &mut off, &mut size));
        assert!(off > 0 && size > 0);
        gm_info_free(info);
    }
}

#[test]
fn unknown_or_empty_files_are_not_found_and_bad_arguments_are_refused() {
    assert_eq!(read_bytes(&[], NAME), (GmStatus::NotFound, null_mut()));
    assert_eq!(read_bytes(&gamemeta_fixtures::sample_3ds(), c"notes.txt".as_ptr()).0, GmStatus::NotFound);
    assert_eq!(read_bytes(&[1, 2], null()).0, GmStatus::ErrArgument);
    let bad_utf8 = [b'a', 0xFF, 0];
    assert_eq!(read_bytes(&[1, 2], bad_utf8.as_ptr().cast()).0, GmStatus::ErrArgument);
    unsafe {
        assert_eq!(gm_read_bytes(null(), 4, NAME, &mut null_mut()), GmStatus::ErrArgument);
        assert_eq!(gm_read_bytes(null(), 0, NAME, null_mut()), GmStatus::ErrArgument);
        assert_eq!(gm_read_callback(10, None, null_mut(), NAME, &mut null_mut()), GmStatus::ErrArgument);
        // Null handles are safe everywhere.
        gm_info_free(null_mut());
        assert_eq!(gm_info_platform(null()), 0);
        assert!(gm_info_title(null(), 1, GmTitleField::Name as u32).is_null());
        assert!(!gm_info_banner(null(), null_mut(), null_mut()));
    }
}

unsafe extern "C" fn from_vec(user: *mut c_void, offset: u64, buf: *mut u8, len: usize) -> i32 {
    let bytes = unsafe { &*(user as *const Vec<u8>) };
    let Some(src) = bytes.get(offset as usize..offset as usize + len) else { return GmReadResult::OutOfRange as i32 };
    unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), buf, len) };
    GmReadResult::Ok as i32
}

unsafe extern "C" fn failing(_: *mut c_void, _: u64, _: *mut u8, _: usize) -> i32 {
    GmReadResult::Error as i32
}

#[test]
fn a_callback_source_reads_and_a_failing_one_is_an_io_error() {
    let bytes = gamemeta_fixtures::sample_3ds();
    let user = &bytes as *const Vec<u8> as *mut c_void;
    let mut out = null_mut();
    unsafe {
        assert_eq!(gm_read_callback(bytes.len() as u64, Some(from_vec), user, NAME, &mut out), GmStatus::Ok);
        assert_eq!(text(gm_info_title(out, 0, GmTitleField::ShortName as u32)).as_deref(), Some("サンプル"));
        gm_info_free(out);
        assert_eq!(gm_read_callback(1 << 20, Some(failing), null_mut(), NAME, &mut out), GmStatus::ErrIo);
        assert!(out.is_null());
    }
}

#[cfg(unix)]
#[test]
fn a_file_descriptor_reads_without_moving_or_closing_it() {
    use std::io::{Seek, SeekFrom};
    use std::os::fd::AsRawFd;
    let path = std::env::temp_dir().join(format!("gamemeta-fd-{}.3ds", std::process::id()));
    std::fs::write(&path, gamemeta_fixtures::sample_3ds()).unwrap();
    let mut file = std::fs::File::open(&path).unwrap();
    file.seek(SeekFrom::Start(7)).unwrap();
    let mut out = null_mut();
    assert_eq!(unsafe { gm_read_fd(file.as_raw_fd(), NAME, &mut out) }, GmStatus::Ok);
    unsafe { gm_info_free(out) };
    assert_eq!(file.stream_position().unwrap(), 7);
    assert_eq!(unsafe { gm_read_fd(-1, NAME, &mut out) }, GmStatus::ErrArgument);
    drop(file);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn versions() {
    assert_eq!(gm_abi_version(), GM_ABI_VERSION);
    assert_eq!(text(gm_version()).as_deref(), Some(env!("CARGO_PKG_VERSION")));
}

#[test]
fn a_ds_rom_gives_ids_animation_and_the_banner_block() {
    let (status, info) = read_bytes(&gamemeta_fixtures::nds::sample_nds(), c"sample.nds".as_ptr());
    assert_eq!(status, GmStatus::Ok);
    unsafe {
        assert_eq!(gm_info_platform(info), GmPlatform::Dsi as u32);
        assert_eq!(text(gm_info_id(info, GmId::GameId as u32)).as_deref(), Some("SMPL"));
        assert_eq!(text(gm_info_id(info, GmId::MakerCode as u32)).as_deref(), Some("01"));
        assert_eq!(text(gm_info_id(info, GmId::TitleId as u32)), None);
        assert_eq!(gm_info_animation_count(info), 2);
        let (mut w, mut h, mut d) = (0, 0, 0);
        assert!(!gm_info_animation_frame(info, 1, &mut w, &mut h, &mut d).is_null());
        assert_eq!((w, h, d), (32, 32, 8));
        assert!(gm_info_animation_frame(info, 2, &mut w, &mut h, &mut d).is_null());
        assert_eq!((w, d), (0, 0));
        let (mut off, mut size) = (0, 0);
        assert!(gm_info_banner(info, &mut off, &mut size));
        assert_eq!((off, size), (0x4000, 0x23C0));
        assert_eq!(gm_info_images(info), 0);
        assert_eq!(gm_info_boot_asset(info, GmBootAsset::Movie as u32), -1);
        gm_info_free(info);
    }
    // The zipped ROM reads the same through the same entry point.
    let zip = gamemeta_fixtures::zip::zip(&[("a.nds", &gamemeta_fixtures::nds::sample_nds(), true)]);
    let (status, info) = read_bytes(&zip, c"sample.zip".as_ptr());
    assert_eq!(status, GmStatus::Ok);
    unsafe { gm_info_free(info) };
}

#[test]
fn a_gamecube_rvz_gives_titles_the_banner_and_the_header_title() {
    let (status, info) = read_bytes(&gamemeta_fixtures::gc::sample_rvz(), c"sample-gc.rvz".as_ptr());
    assert_eq!(status, GmStatus::Ok);
    unsafe {
        assert_eq!(gm_info_platform(info), GmPlatform::GameCube as u32);
        assert_eq!(text(gm_info_title(info, 1, GmTitleField::Name as u32)).as_deref(), Some("Sample Quest"));
        assert_eq!(text(gm_info_id(info, GmId::GameId as u32)).as_deref(), Some("GSMP01"));
        assert_eq!(text(gm_info_header_title(info)).as_deref(), Some("Sample Disc"));
        assert_eq!(gm_info_images(info), 1 << 1);
        let (mut w, mut h) = (0, 0);
        assert!(!gm_info_icon(info, 0, &mut w, &mut h).is_null());
        assert_eq!((w, h), (96, 32));
        gm_info_free(info);
    }
}

#[test]
fn a_wii_rvz_gives_the_logo_and_a_wad_its_title_id() {
    let (status, info) = read_bytes(&gamemeta_fixtures::wii::sample_rvz(), c"sample-wii.rvz".as_ptr());
    assert_eq!(status, GmStatus::Ok);
    unsafe {
        assert_eq!(gm_info_platform(info), GmPlatform::Wii as u32);
        assert_eq!(text(gm_info_title(info, 1, GmTitleField::Name as u32)).as_deref(), Some("Sample Channel"));
        assert_eq!(gm_info_images(info), 1 << 2);
        let (mut w, mut h) = (0, 0);
        assert!(!gm_info_icon(info, 0, &mut w, &mut h).is_null());
        assert_eq!((w, h), (64, 16));
        gm_info_free(info);
    }
    let (status, info) = read_bytes(&gamemeta_fixtures::wii::sample_wad(), c"sample.wad".as_ptr());
    assert_eq!(status, GmStatus::Ok);
    unsafe {
        assert_eq!(
            text(gm_info_id(info, GmId::TitleId as u32)).as_deref(),
            Some("00010001 5253 4D50".replace(' ', "").as_str())
        );
        assert_eq!(text(gm_info_id(info, GmId::GameId as u32)).as_deref(), Some("RSMP"));
        gm_info_free(info);
    }
}

use gamemeta_fixtures::wiiu as wiiu_fx;

type Files = std::collections::HashMap<String, Vec<u8>>;

/// The callback of a title held in a map: size query when `buf` is null, copy when `cap` is enough.
unsafe extern "C" fn from_map(user: *mut c_void, path: *const c_char, buf: *mut u8, cap: usize, size: *mut u64) -> i32 {
    let files = unsafe { &*(user as *const Files) };
    let path = unsafe { CStr::from_ptr(path) }.to_str().unwrap();
    let Some(data) = files.get(path) else { return GmFileResult::NotFound as i32 };
    unsafe { *size = data.len() as u64 };
    if !buf.is_null() && cap >= data.len() {
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), buf, data.len()) };
    }
    GmFileResult::Ok as i32
}

unsafe extern "C" fn failing_files(_: *mut c_void, _: *const c_char, _: *mut u8, _: usize, _: *mut u64) -> i32 {
    GmFileResult::Error as i32
}

fn standard_title() -> Files {
    wiiu_fx::standard_files(&wiiu_fx::Meta::default().xml()).into_iter().collect()
}

#[test]
fn a_wii_u_title_reads_through_the_file_callback_with_images_on_demand() {
    let files = standard_title();
    let user = &files as *const Files as *mut c_void;
    let mut out = null_mut();
    unsafe {
        assert_eq!(gm_read_title_files(Some(from_map), user, &mut out), GmStatus::Ok);
        assert_eq!(gm_info_platform(out), GmPlatform::WiiU as u32);
        assert_eq!(text(gm_info_title(out, 1, GmTitleField::Name as u32)).as_deref(), Some("Test Quest"));
        assert_eq!(text(gm_info_id(out, GmId::TitleId as u32)).as_deref(), Some("0005000010123456"));
        assert_eq!(text(gm_info_id(out, GmId::ProductCode as u32)).as_deref(), Some("WUP-P-ABCE"));
        assert_eq!(gm_info_regions(out), 0b10);
        assert_eq!(gm_info_images(out), 0b111001);
        assert_eq!(gm_info_boot_asset(out, GmBootAsset::Movie as u32), 1234);
        assert_eq!(gm_info_boot_asset(out, GmBootAsset::Sound as u32), 99);
        let (mut w, mut h) = (0, 0);
        assert!(!gm_info_icon(out, GmIcon::Large as u32, &mut w, &mut h).is_null());
        assert_eq!((w, h), (4, 2));
        gm_info_free(out);

        let mut pixels = null_mut();
        assert_eq!(gm_title_image(Some(from_map), user, GmImage::TvSplash as u32, &mut pixels), GmStatus::Ok);
        let data = gm_pixels_data(pixels, &mut w, &mut h);
        assert_eq!((w, h), (16, 9));
        assert_eq!(std::slice::from_raw_parts(data, 4), [3, 2, 1, 255]);
        gm_pixels_free(pixels);
        let mut none = null_mut();
        assert_eq!(gm_title_image(Some(from_map), user, GmImage::Banner as u32, &mut none), GmStatus::ErrArgument);
        assert!(none.is_null());
        assert_eq!(gm_pixels_data(null(), &mut w, &mut h), null());
        assert_eq!((w, h), (0, 0));
        gm_pixels_free(null_mut());
    }
}

#[test]
fn a_wii_u_title_with_no_meta_is_not_found_a_failing_callback_is_an_io_error_and_bad_arguments_are_refused() {
    let mut files = standard_title();
    files.remove("meta/meta.xml");
    files.remove("meta/bootLogoTex.tga");
    let user = &files as *const Files as *mut c_void;
    let mut out = null_mut();
    unsafe {
        assert_eq!(gm_read_title_files(Some(from_map), user, &mut out), GmStatus::NotFound);
        assert!(out.is_null());
        assert_eq!(gm_read_title_files(Some(failing_files), null_mut(), &mut out), GmStatus::ErrIo);
        assert_eq!(gm_read_title_files(None, user, &mut out), GmStatus::ErrArgument);
        assert_eq!(gm_read_title_files(Some(from_map), user, null_mut()), GmStatus::ErrArgument);
        let mut pixels = null_mut();
        assert_eq!(gm_title_image(Some(from_map), user, GmImage::BootLogo as u32, &mut pixels), GmStatus::NotFound);
        assert_eq!(gm_title_image(Some(failing_files), null_mut(), 0, &mut pixels), GmStatus::ErrIo);
        assert_eq!(gm_title_image(None, user, 0, &mut pixels), GmStatus::ErrArgument);
    }
}

#[test]
fn a_wua_reads_through_the_ordinary_entry_points_and_its_splashes_on_demand() {
    let wua = wiiu_fx::sample_wua();
    let (status, info) = read_bytes(&wua, c"games.wua".as_ptr());
    assert_eq!(status, GmStatus::Ok);
    unsafe {
        assert_eq!(gm_info_platform(info), GmPlatform::WiiU as u32);
        assert_eq!(text(gm_info_title(info, 1, GmTitleField::Name as u32)).as_deref(), Some("Sample Title"));
        assert_eq!(text(gm_info_title(info, 0, GmTitleField::Name as u32)).as_deref(), Some("サンプル"));
        assert_eq!(text(gm_info_id(info, GmId::TitleId as u32)).as_deref(), Some("00050000101ABCDE"));
        assert_eq!(gm_info_regions(info), 0b100);
        gm_info_free(info);

        let user = &wua as *const Vec<u8> as *mut c_void;
        let mut pixels = null_mut();
        let status = gm_wua_image(wua.len() as u64, Some(from_vec), user, GmImage::GamePadSplash as u32, &mut pixels);
        assert_eq!(status, GmStatus::Ok);
        let (mut w, mut h) = (0, 0);
        assert!(!gm_pixels_data(pixels, &mut w, &mut h).is_null());
        assert_eq!((w, h), (8, 5));
        gm_pixels_free(pixels);
        assert_eq!(gm_wua_image(1 << 20, Some(failing), null_mut(), 3, &mut pixels), GmStatus::ErrIo);
        assert_eq!(gm_wua_image(10, None, null_mut(), 3, &mut pixels), GmStatus::ErrArgument);
    }
}

#[cfg(unix)]
#[test]
fn a_wua_in_a_file_descriptor_decodes_its_splash_and_a_bad_descriptor_is_refused() {
    use std::os::fd::AsRawFd;
    let path = std::env::temp_dir().join(format!("gamemeta-wua-{}.wua", std::process::id()));
    std::fs::write(&path, wiiu_fx::sample_wua()).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    let image = gamemeta::fd::wua_image_fd(file.as_raw_fd(), gamemeta_core::ImageKind::BootLogo).unwrap();
    assert_eq!((image.width, image.height), (2, 2));
    assert_eq!(gamemeta::fd::wua_image_fd(file.as_raw_fd(), gamemeta_core::ImageKind::Banner), Err(GmStatus::NotFound));
    assert_eq!(gamemeta::fd::wua_image_fd(-1, gamemeta_core::ImageKind::Icon), Err(GmStatus::ErrArgument));
    drop(file);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn cartridge_roms_give_platform_version_ids_and_the_header_json() {
    use gamemeta_fixtures::carts as fx;
    let cases: Vec<(&CStr, Vec<u8>, GmPlatform, i64, &str)> = vec![
        (c"a.gb", fx::gb::Spec::default().build(), GmPlatform::GameBoy, 2, "gb"),
        (c"a.gba", fx::gba::Spec::default().build(), GmPlatform::GameBoyAdvance, 1, "gba"),
        (c"a.z64", fx::n64::Spec::default().build(), GmPlatform::Nintendo64, 1, "n64"),
        (c"a.sfc", fx::snes::Spec::new(fx::snes::Layout::Lo).build(), GmPlatform::Snes, 0, "snes"),
        (c"a.nes", fx::nes::Spec { prg: 1, ..Default::default() }.build(), GmPlatform::Nes, -1, "nes"),
        (c"a.md", fx::md::Spec::default().build(), GmPlatform::MegaDrive, 2, "md"),
        (c"a.sms", fx::sms::Spec::default().build(), GmPlatform::MasterSystem, 1, "sms"),
        (c"a.gg", fx::sms::Spec { region: 6, ..Default::default() }.build(), GmPlatform::GameGear, 1, "sms"),
    ];
    for (name, bytes, platform, version, family) in cases {
        let (status, info) = read_bytes(&bytes, name.as_ptr());
        assert_eq!(status, GmStatus::Ok, "{name:?}");
        unsafe {
            assert_eq!(gm_info_platform(info), platform as u32, "{name:?}");
            assert_eq!(gm_info_version(info), version, "{name:?}");
            let json = text(gm_info_cart_json(info)).expect("a header");
            assert!(json.starts_with(&format!("{{\"family\":\"{family}\"")), "{json}");
            gm_info_free(info);
        }
    }
}

#[test]
fn a_game_boy_rom_gives_title_maker_and_regions_through_the_usual_accessors() {
    let rom = gamemeta_fixtures::carts::gb::Spec::default().build();
    let (status, info) = read_bytes(&rom, c"a.gb".as_ptr());
    assert_eq!(status, GmStatus::Ok);
    unsafe {
        assert_eq!(text(gm_info_title(info, 1, GmTitleField::Name as u32)).as_deref(), Some("SYNTH TEST"));
        assert_eq!(text(gm_info_id(info, GmId::MakerCode as u32)).as_deref(), Some("01"));
        assert_eq!(gm_info_regions(info), (1 << 1) | (1 << 2));
        let json = text(gm_info_cart_json(info)).unwrap();
        assert!(json.contains("\"title\":\"SYNTH TEST\"") && json.contains("\"logo_valid\":true"), "{json}");
        gm_info_free(info);
    }
}

#[test]
fn a_file_without_a_cartridge_header_has_no_version_and_no_header_json() {
    let (status, info) = read_bytes(&gamemeta_fixtures::sample_3ds(), NAME);
    assert_eq!(status, GmStatus::Ok);
    unsafe {
        assert_eq!(gm_info_version(info), -1);
        assert!(gm_info_cart_json(info).is_null());
        assert_eq!(gm_info_version(null()), -1);
        assert!(gm_info_cart_json(null()).is_null());
        gm_info_free(info);
    }
}
