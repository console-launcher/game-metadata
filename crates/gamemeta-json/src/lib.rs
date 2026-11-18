// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A small JSON writer for `GameInfo` and `Ids` (no dependencies): the CLI prints it, the wasm binding hands it to the page.

use std::fmt::Write;

use gamemeta_core::ids::Ids;
use gamemeta_core::{GameInfo, RgbaImage};

/// `s` as a JSON string literal appended to `out`.
pub fn string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn opt_string(out: &mut String, s: Option<&str>) {
    match s {
        Some(s) => string(out, s),
        None => out.push_str("null"),
    }
}

fn image(out: &mut String, im: Option<&RgbaImage>) {
    match im {
        Some(im) => {
            let _ = write!(out, "{{\"width\":{},\"height\":{}}}", im.width, im.height);
        }
        None => out.push_str("null"),
    }
}

/// `info` as one line of JSON; `null` for `None`.
/// The summary line of `gamemeta wii-banner`: animation lengths, frame size at scale 1, panes skipped and textures missing.
pub fn wii_banner(banner: &gamemeta_core::wii::banner::WiiBanner) -> String {
    let (w, h) = banner.pixel_size(1.0).unwrap_or((0, 0));
    let mut out = format!(
        "{{\"startFrames\":{},\"loopFrames\":{},\"size\":[{w},{h}],\"skipped\":{{",
        banner.start_frames(),
        banner.loop_frames()
    );
    for (i, (kind, n)) in banner.skipped().iter().enumerate() {
        let _ = write!(out, "{}\"{kind}\":{n}", if i > 0 { "," } else { "" });
    }
    out.push_str("},\"missingTextures\":[");
    for (i, t) in banner.missing_textures().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        string(&mut out, t);
    }
    out.push_str("]}");
    out
}

pub fn info(info: Option<&GameInfo>) -> String {
    let Some(i) = info else { return "null".into() };
    let mut o = String::new();
    let _ = write!(o, "{{\"platform\":\"{:?}\",\"title\":", i.platform);
    opt_string(&mut o, i.title());
    o.push_str(",\"titles\":{");
    for (n, (lang, t)) in i.titles.iter().enumerate() {
        if n > 0 {
            o.push(',');
        }
        let _ = write!(o, "\"{lang:?}\":{{\"name\":");
        string(&mut o, &t.name);
        o.push_str(",\"shortName\":");
        string(&mut o, &t.short_name);
        o.push_str(",\"publisher\":");
        opt_string(&mut o, t.publisher.as_deref());
        o.push('}');
    }
    o.push_str("},\"titleId\":");
    opt_string(&mut o, i.ids.title_id.as_deref());
    o.push_str(",\"productCode\":");
    opt_string(&mut o, i.ids.product_code.as_deref());
    o.push_str(",\"gameId\":");
    opt_string(&mut o, i.ids.game_id.as_deref());
    o.push_str(",\"makerCode\":");
    opt_string(&mut o, i.ids.maker_code.as_deref());
    o.push_str(",\"headerTitle\":");
    opt_string(&mut o, i.header_title.as_deref());
    o.push_str(",\"icon\":");
    image(&mut o, i.icon.as_ref());
    o.push_str(",\"smallIcon\":");
    image(&mut o, i.small_icon.as_ref());
    let steps: Vec<String> = i.animated_icon.iter().map(|s| s.duration_frames.to_string()).collect();
    let _ = write!(o, ",\"animation\":[{}]", steps.join(","));
    let images: Vec<String> = i.images.iter().map(|k| format!("\"{k:?}\"")).collect();
    let _ = write!(o, ",\"images\":[{}]", images.join(","));
    for (key, size) in [("bootMovie", i.boot_movie), ("bootSound", i.boot_sound)] {
        match size {
            Some(n) => {
                let _ = write!(o, ",\"{key}\":{n}");
            }
            None => {
                let _ = write!(o, ",\"{key}\":null");
            }
        }
    }
    let regions: Vec<String> = i.regions.iter().map(|r| format!("\"{r:?}\"")).collect();
    let _ = write!(o, ",\"regions\":[{}],\"regionFree\":{},\"ratings\":{{", regions.join(","), i.region_free);
    for (n, (board, age)) in i.ratings.iter().enumerate() {
        let _ = write!(o, "{}\"{board:?}\":{age}", if n > 0 { "," } else { "" });
    }
    o.push_str("},\"banner\":");
    match i.banner {
        Some(b) => {
            let _ = write!(o, "{{\"offset\":{},\"size\":{}}}", b.offset, b.size);
        }
        None => o.push_str("null"),
    }
    o.push_str(",\"version\":");
    match i.version {
        Some(v) => {
            let _ = write!(o, "{v}");
        }
        None => o.push_str("null"),
    }
    o.push_str(",\"cart\":");
    match &i.cart {
        Some(c) => o.push_str(&c.to_json()),
        None => o.push_str("null"),
    }
    o.push('}');
    o
}

/// `ids` as one line of JSON (every field, `null` where there is none, plus `primaryId`); `null` for `None`.
pub fn ids(ids: Option<&Ids>) -> String {
    let Some(i) = ids else { return "null".into() };
    let mut o = String::from("{");
    let fields = [
        ("serial", i.serial.as_deref()),
        ("gameId", i.game_id.as_deref()),
        ("titleId", i.title_id.as_deref()),
        ("chdSha1", i.chd_sha1.as_deref()),
        ("zipCrc", i.zip_crc.as_deref()),
        ("pspTitle", i.psp_title.as_deref()),
        ("pspDiscId", i.psp_disc_id.as_deref()),
        ("xboxId", i.xbox_id.as_deref()),
        ("raMd5", i.ra_md5.as_deref()),
        ("primaryId", i.primary_id()),
    ];
    for (n, (key, value)) in fields.into_iter().enumerate() {
        if n > 0 {
            o.push(',');
        }
        let _ = write!(o, "\"{key}\":");
        opt_string(&mut o, value);
        if key == "zipCrc" {
            let _ = match i.zip_size {
                Some(size) => write!(o, ",\"zipSize\":{size}"),
                None => write!(o, ",\"zipSize\":null"),
            };
        }
    }
    o.push('}');
    o
}

#[cfg(test)]
mod tests {
    use gamemeta_core::SliceSource;

    #[test]
    fn the_sample_as_json() {
        let info = gamemeta_core::read(&mut SliceSource::new(&gamemeta_fixtures::sample_3ds()), "s.3ds");
        let json = super::info(info.as_ref());
        assert!(json.starts_with(r#"{"platform":"ThreeDs","title":"Sample Game Subtitle","titles":{"Japanese":"#));
        assert!(
            json.contains(r#""titleId":"0004000000123400","productCode":"CTR-P-SMPL","gameId":null,"makerCode":null,"headerTitle":null,"icon":{"width":48,"height":48}"#)
        );
        assert!(json.contains(
            r#""animation":[],"images":[],"bootMovie":null,"bootSound":null,"regions":["NorthAmerica","Europe"],"regionFree":false,"ratings":{"Cero":12,"Esrb":10,"Pegi":0}"#
        ));
        assert_eq!(super::info(None), "null");
    }

    #[test]
    fn a_ds_sample_as_json() {
        let nds = gamemeta_fixtures::nds::sample_nds();
        let json = super::info(gamemeta_core::read(&mut SliceSource::new(&nds), "s.nds").as_ref());
        assert!(json.starts_with(r#"{"platform":"Dsi","title":"Sample Game Subtitle","titles":{"English":"#));
        assert!(
            json.contains(r#""gameId":"SMPL","makerCode":"01","headerTitle":null,"icon":{"width":32,"height":32}"#)
        );
        assert!(json.contains(r#""animation":[10,8]"#));
    }

    #[test]
    fn strings_are_escaped() {
        let mut s = String::new();
        super::string(&mut s, "a\"b\\c\n\u{1}");
        assert_eq!(s, r#""a\"b\\c\n\u0001""#);
    }
}
