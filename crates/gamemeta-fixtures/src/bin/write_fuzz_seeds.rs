// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Writes seed inputs for the fuzz targets: `write-fuzz-seeds <corpus dir>` (one subdirectory per target).
use gamemeta_fixtures::*;

fn write(dir: &std::path::Path, target: &str, seeds: Vec<(&str, Vec<u8>)>) {
    let dir = dir.join(target);
    std::fs::create_dir_all(&dir).expect("create the seed directory");
    for (name, bytes) in seeds {
        std::fs::write(dir.join(name), bytes).expect("write a seed");
    }
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "fuzz/corpus".into()));
    let icon = smdh("Seed", "Seed Title", "Pub", 0xF800);
    let banner = cbmd(&[1; 32], &[]);
    let files: &[(&str, &[u8])] = &[("banner", &banner), ("icon", &icon)];
    write(
        &dir,
        "threeds_walk",
        vec![
            ("sample.3ds", sample_3ds()),
            ("ncch.cxi", plain_ncch(&exefs(files))),
            ("ncsd.3ds", ncsd(&plain_ncch(&exefs(files)))),
            ("cia.cia", cia(&plain_ncch(&exefs(files)))),
            ("ncch-icon-only.cxi", plain_ncch(&exefs(&[("icon", &icon)]))),
        ],
    );
    let rom = nds::rom(&nds::banner(3, "Seed\nSub\nMaker", &[]), 0x200, 0x40);
    write(
        &dir,
        "nds_walk",
        vec![
            ("sample.nds", nds::sample_nds()),
            ("v3.nds", rom.clone()),
            ("stored.zip", nds::zip("a.nds", &rom, false)),
            ("deflated.zip", nds::zip("a.nds", &rom, true)),
            ("banner-v103.bin", nds::banner(0x0103, "T", &[0x0A | (1 << 8), 0])),
        ],
    );
    let logo = wii::tpl_plain(64, 16, 5, &wii::words16(64, 16, |_, _| 0x4F00));
    let wii_icon = wii::bin_archive(&logo, "Logo.tpl", false, false, &[]);
    let wii_icon_lz = wii::bin_archive(&logo, "Logo.tpl", true, true, &[]);
    let wii_bnr = wii::opening_plain(&[("en", "Seed"), ("ja", "シード")], Some(&wii_icon), None);
    let bnr2 = gc::bnr(&["A", "B", "C", "D", "E", "F"].map(gc::text), &gc::image(), "windows-1252");
    let gc_disc = gc::standard_disc(Some(&bnr2), "GSMP01", Some(&gc::bnr1("Decoy")));
    let rvz_opts = |compression, chunk, packed| gc::RvzOptions { compression, chunk, packed, ..Default::default() };
    write(
        &dir,
        "gc_walk",
        vec![
            ("disc.iso", gc_disc.clone()),
            ("disc.ciso", gc::ciso(&gc_disc, 0x8000)),
            ("disc.gcz", gc::gcz(&gc_disc, 0x8000)),
            ("zstd.rvz", gc::rvz(&gc_disc, rvz_opts(5, 0x8000, false), b"RVZ\x01")),
            ("stored.rvz", gc::rvz(&gc_disc, rvz_opts(0, 0x20000, false), b"RVZ\x01")),
            ("packed.rvz", gc::rvz(&gc_disc, rvz_opts(5, 0x8000, true), b"RVZ\x01")),
            ("sample.rvz", gc::sample_rvz()),
        ],
    );
    let partition = wii::partition(&[wii::PartFile("opening.bnr", &wii_bnr)], Some(&[5u8; 0x700]));
    let wii_opts = |compression, chunk, packed, exceptions| wii::WiiRvzOptions {
        compression,
        chunk,
        packed,
        exceptions,
        ..Default::default()
    };
    write(
        &dir,
        "wii_walk",
        vec![
            ("sample.rvz", wii::sample_rvz()),
            ("stored.rvz", wii::rvz(&partition, &wii_opts(0, 0x20000, false, true)).bytes),
            ("packed.rvz", wii::rvz(&partition, &wii_opts(5, 0x8000, true, true)).bytes),
            ("big-chunk.rvz", wii::rvz(&partition, &wii_opts(5, 0x40_0000, false, false)).bytes),
            ("disc.iso", wii::disc_header("RZDE01", "Seed", 0x60000)),
            ("disc.wbfs", wii::wbfs("RZDP01", "Seed")),
            ("channel.wad", wii::sample_wad()),
            ("opening.bnr", wii_bnr.clone()),
            ("opening-lz77.bnr", wii::opening(&[("en", "T")], Some(&wii_icon_lz), None, |a| a)),
        ],
    );
    let meta = wiiu::Meta::default().xml();
    let noisy = {
        let mut x = 1u32;
        let noise: Vec<u8> = (0..150_000)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (x >> 24) as u8
            })
            .collect();
        let mut files: Vec<(String, Vec<u8>)> =
            wiiu::standard_files(&meta).into_iter().map(|(p, d)| (format!("0005000010123456_v0/{p}"), d)).collect();
        files.push(("0005000010123456_v0/content/noise.bin".into(), noise));
        files
    };
    write(
        &dir,
        "wiiu_walk",
        vec![
            ("sample.wua", wiiu::sample_wua()),
            ("noisy.wua", wiiu::zarchive(&noisy)),
            ("meta.xml", meta.clone().into_bytes()),
            (
                "cdata.xml",
                b"<menu><longname_en><![CDATA[A <b> & C]]></longname_en><title_id>&#x41;</title_id></menu>".to_vec(),
            ),
            ("icon.tga", wiiu::tga(4, 2, 32, false, false, |x, y| [x as u8, y as u8, 3, 255])),
            ("icon-rle.tga", wiiu::tga(6, 4, 24, true, true, |_, y| [1, y as u8, 3, 255])),
        ],
    );
    // ids_walk: the first byte picks the file name (the index into the target's name list), the rest is the file.
    let pick = |index: u8, bytes: Vec<u8>| [vec![index], bytes].concat();
    let ps2 = ids::sample_ps2_iso();
    write(
        &dir,
        "ids_walk",
        vec![
            ("ps2.iso", pick(0, ps2.clone())),
            ("psp.iso", pick(0, ids::sample_psp_iso())),
            ("gc.iso", pick(0, ids::sample_gc_iso())),
            ("gc.wbfs", pick(4, ids::wbfs(&ids::game_cube("GSMP01")))),
            ("gc.rvz", pick(2, ids::rvz(&ids::game_cube("GSMP01"), b"RVZ\x01"))),
            ("gc.ciso", pick(5, ids::ciso(&ids::game_cube("GSMP01")))),
            ("ps2.bin", pick(7, ids::raw2352(&ps2, 2))),
            ("ps1.bin", pick(7, ids::raw2352(&ps2, 1))),
            ("sample.cue", pick(9, ids::sample_cue().into_bytes())),
            ("sample.pbp", pick(10, ids::sample_pbp())),
            ("sample.nsp", pick(14, ids::sample_nsp())),
            ("meta.rpx", pick(16, vec![0; 16])),
            ("sample.xbe", pick(17, ids::sample_xbe())),
            ("sample.zip", pick(18, ids::sample_rom_zip())),
            ("sample.chd", pick(19, ids::sample_chd())),
            ("sample.nes", pick(20, ids::sample_nes())),
            ("sample.sfc", pick(21, ids::sample_rom(4096))),
            ("sample.smc", pick(22, [vec![0; 512], ids::sample_rom(4096)].concat())),
            ("sample.gb", pick(23, ids::sample_rom(2048))),
            ("sample.wad", pick(11, wii::sample_wad())),
            ("sample.3ds", pick(12, sample_3ds())),
        ],
    );
    // carts: the first byte picks the name (index into the target's NAMES), then the file.
    let pick_cart = |index: u8, bytes: Vec<u8>| [vec![index], bytes].concat();
    let snes_lo = carts::snes::Spec::new(carts::snes::Layout::Lo).build();
    let md = carts::md::Spec::default().build();
    write(
        &dir,
        "carts",
        vec![
            ("gb", pick_cart(0, carts::gb::Spec::default().build())),
            (
                "gbc",
                pick_cart(1, carts::gb::Spec { cgb: 0x80, manufacturer: Some("SYNT"), ..Default::default() }.build()),
            ),
            ("gba", pick_cart(3, carts::gba::Spec::default().build())),
            ("z64", pick_cart(5, carts::n64::Spec::default().build())),
            ("v64", pick_cart(6, carts::n64::to_v64(&carts::n64::Spec::default().build()))),
            ("n64", pick_cart(7, carts::n64::to_n64(&carts::n64::Spec::default().build()))),
            ("sfc", pick_cart(8, snes_lo.clone())),
            (
                "smc",
                pick_cart(
                    9,
                    carts::snes::Spec { copier: true, ..carts::snes::Spec::new(carts::snes::Layout::Hi) }.build(),
                ),
            ),
            ("nes", pick_cart(10, carts::nes::Spec { prg: 2, chr: 1, mapper: 4, ..Default::default() }.build())),
            (
                "nes2",
                pick_cart(
                    10,
                    carts::nes::Spec {
                        nes2: true,
                        prg: 0x123,
                        chr: 5,
                        ram_shifts: [7, 8, 9, 10],
                        ..Default::default()
                    }
                    .build(),
                ),
            ),
            ("md", pick_cart(11, md.clone())),
            ("smd", pick_cart(14, carts::md::to_smd(&md))),
            ("sms", pick_cart(16, carts::sms::Spec::default().build())),
            ("gg", pick_cart(17, carts::sms::Spec { region: 6, ..Default::default() }.build())),
            ("zip", pick_cart(18, zip::zip(&[("a.sfc", &snes_lo, true)]))),
        ],
    );
    // The scene corpus: every committed banner and CGFX input (fixtures/scene), which the dump goldens already cover.
    let scene_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/scene");
    let mut scene_seeds = Vec::new();
    for sub in ["features", "cases"] {
        let mut entries: Vec<_> = std::fs::read_dir(scene_dir.join(sub))
            .expect("read fixtures/scene")
            .map(|e| e.expect("entry").path())
            .collect();
        entries.sort();
        for path in entries.into_iter().filter(|p| p.extension().is_some_and(|e| e == "cbmd" || e == "cgfx")) {
            let name = format!("{sub}-{}", path.file_name().expect("name").to_string_lossy());
            scene_seeds.push((name, std::fs::read(&path).expect("read a scene input")));
        }
    }
    write(&dir, "banner3ds_scene", scene_seeds.iter().map(|(n, b)| (n.as_str(), b.clone())).collect());
    // banner3ds_render: width, height (1 to 64 each) and the frame (a byte, in frames) ahead of the same inputs.
    let render_seeds: Vec<_> = scene_seeds
        .iter()
        .enumerate()
        .map(|(i, (n, b))| {
            let mut input = vec![40, 24, (i * 37 % 256) as u8];
            input.extend_from_slice(b);
            (n.as_str(), input)
        })
        .collect();
    write(&dir, "banner3ds_render", render_seeds);
    // wii_banner: scale and frame bytes ahead of every authored banner archive (fixtures/wii-banner's own cases), plus the same
    // archives wrapped (LZ77, IMD5) and inside an opening.bnr, which the file readers take.
    let banner_seeds: Vec<(String, Vec<u8>)> = gamemeta_fixtures::wii_banner_cases::cases()
        .into_iter()
        .enumerate()
        .flat_map(|(i, c)| {
            let head = [(i * 7 % 64) as u8, (i * 29 % 256) as u8];
            let wrapped = gamemeta_fixtures::wii_banner::lz77_wrapped(&c.archive);
            let bnr = wii::opening(&[("en", "Seed")], None, Some(&c.archive), |a| a);
            vec![
                (c.name.to_string(), [&head[..], &c.archive].concat()),
                (format!("{}-lz77", c.name), [&head[..], &wrapped].concat()),
                (format!("{}-bnr", c.name), [&head[..], &bnr].concat()),
            ]
        })
        .collect();
    write(&dir, "wii_banner", banner_seeds.iter().map(|(n, b)| (n.as_str(), b.clone())).collect());
}
