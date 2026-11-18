// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The authored banners the original renderer's tests are built from (and its throwaway oracle rendered into
//! `fixtures/wii-banner`): each case is a banner archive (`meta/banner.bin`, plain U8) and the frames `(frame, scale)` that were
//! rendered. A test checks that these writers give the original implementation's bytes.

use crate::wii_banner::{
    BLUE5, GREEN5, Key, Mat, Pane, RED5, Tag, Target, Tex, animation, archive, layout, quadrants, tpl_opaque,
    tpl_words, track,
};

/// A named authored banner.
#[derive(Debug)]
pub struct Case {
    /// File stem under `fixtures/wii-banner`.
    pub name: &'static str,
    /// The banner archive bytes.
    pub archive: Vec<u8>,
    /// The `(frame, scale)` pairs rendered, in order.
    pub frames: Vec<(f32, f32)>,
}

const RED: u32 = 0xFF00_00FF;
const GREEN: u32 = 0x00FF_00FF;
const BLUE: u32 = 0x0000_FFFF;
const YELLOW: u32 = 0xFFFF_00FF;

fn flat(panes: &[Pane]) -> Vec<u8> {
    layout(200.0, 100.0, &[], &[Mat::new("m")], panes)
}

fn red_pane(w: f32, h: f32) -> Pane {
    Pane { w, h, colors: [RED; 4], ..Pane::new("p") }
}

fn small() -> Pane {
    Pane { w: 20.0, h: 20.0, colors: [RED; 4], ..Pane::new("p") }
}

fn slide(looping: bool) -> Vec<u8> {
    animation(
        60,
        looping,
        &[Target {
            name: "p",
            material: false,
            tags: vec![Tag(
                "RLPA",
                vec![track(0, 0, true, &[Key(0.0, -50.0, 100.0 / 60.0), Key(60.0, 50.0, 100.0 / 60.0)])],
            )],
        }],
        &[],
    )
}

fn one(tags: Vec<Tag>, name: &'static str, material: bool, frames: u16, textures: &[&str]) -> Vec<u8> {
    animation(frames, false, &[Target { name, material, tags }], textures)
}

fn k(frame: f32, value: f32) -> Key {
    Key(frame, value, 0.0)
}

struct B {
    layout: Vec<u8>,
    anims: Vec<(&'static str, Vec<u8>)>,
    textures: Vec<(&'static str, Vec<u8>)>,
    frames: Vec<(f32, f32)>,
}

fn b(layout: Vec<u8>) -> B {
    B { layout, anims: Vec::new(), textures: Vec::new(), frames: vec![(0.0, 1.0)] }
}

impl B {
    fn anim(mut self, name: &'static str, bytes: Vec<u8>) -> Self {
        self.anims.push((name, bytes));
        self
    }
    fn tex(mut self, name: &'static str, bytes: Vec<u8>) -> Self {
        self.textures.push((name, bytes));
        self
    }
    fn frames(mut self, f: &[(f32, f32)]) -> Self {
        self.frames = f.to_vec();
        self
    }
    fn case(self, name: &'static str) -> Case {
        let anims: Vec<(&str, &[u8])> = self.anims.iter().map(|(n, d)| (*n, d.as_slice())).collect();
        let textures: Vec<(&str, &[u8])> = self.textures.iter().map(|(n, d)| (*n, d.as_slice())).collect();
        Case { name, archive: archive(&self.layout, &anims, &textures), frames: self.frames }
    }
}

const START: &str = "banner_Start.brlan";
const LOOP: &str = "banner_Loop.brlan";

/// Every authored case, in the order the oracle wrote them.
pub fn cases() -> Vec<Case> {
    let mut l = Vec::new();
    l.push(b(flat(&[red_pane(50.0, 50.0)])).case("pane"));
    l.push(b(flat(&[Pane { origin: 0, w: 50.0, h: 50.0, colors: [RED; 4], ..Pane::new("p") }])).case("origin-tl"));
    l.push(b(flat(&[Pane { origin: 8, w: 50.0, h: 50.0, colors: [RED; 4], ..Pane::new("p") }])).case("origin-br"));
    let names = ["o0", "o1", "o2", "o3", "o4", "o5", "o6", "o7", "o8"];
    let grid: Vec<Pane> = (0..9)
        .map(|i| Pane {
            x: ((i % 3) as f32 - 1.0) * 60.0,
            y: (1.0 - (i / 3) as f32) * 30.0,
            origin: i as u8,
            w: 20.0,
            h: 10.0,
            colors: [RED, GREEN, BLUE, YELLOW],
            ..Pane::new(names[i])
        })
        .collect();
    l.push(b(flat(&grid)).case("origin-grid"));
    let child = Pane { x: 10.0, w: 10.0, h: 10.0, colors: [RED; 4], ..Pane::new("c") };
    l.push(
        b(flat(&[Pane { kind: "pan1", x: 40.0, scale: 2.0, children: vec![child], ..Pane::new("g") }])).case("nested"),
    );
    l.push(b(flat(&[Pane { w: 100.0, h: 20.0, rot_z: 90.0, colors: [RED; 4], ..Pane::new("p") }])).case("rotate"));
    l.push(
        b(flat(&[Pane { w: 80.0, h: 30.0, rot_z: 37.0, colors: [RED; 4], ..Pane::new("p") }]))
            .frames(&[(0.0, 1.0), (0.0, 1.37)])
            .case("rotate-37"),
    );
    l.push(
        b(flat(&[Pane { colors: [0x0000_00FF, 0xFFFF_FFFF, 0x0000_00FF, 0xFFFF_FFFF], ..Pane::new("p") }]))
            .case("gradient"),
    );
    let q_layout =
        || layout(200.0, 100.0, &["q.tpl"], &[Mat { textures: vec![Tex(0, 0)], ..Mat::new("m") }], &[Pane::new("p")]);
    l.push(b(q_layout()).tex("q.tpl", quadrants()).frames(&[(0.0, 1.0), (0.0, 2.5)]).case("quadrants"));
    let kid = |influenced: bool| Pane { w: 40.0, h: 40.0, influenced, colors: [RED; 4], ..Pane::new("c") };
    l.push(
        b(flat(&[Pane { kind: "pan1", alpha: 128, children: vec![kid(true)], ..Pane::new("g") }])).case("alpha-dim"),
    );
    l.push(
        b(flat(&[Pane { kind: "pan1", alpha: 128, children: vec![kid(false)], ..Pane::new("g") }])).case("alpha-full"),
    );
    l.push(
        b(flat(&[Pane { kind: "pan1", visible: false, children: vec![kid(true)], ..Pane::new("g") }]))
            .case("alpha-hidden"),
    );
    l.push(
        b(flat(&[
            Pane { kind: "txt1", ..Pane::new("t") },
            Pane { kind: "wnd1", ..Pane::new("w") },
            Pane { kind: "bnd1", ..Pane::new("b") },
            red_pane(20.0, 20.0),
        ]))
        .case("skipped"),
    );
    l.push(
        b(layout(
            200.0,
            100.0,
            &["gone.tpl"],
            &[Mat { textures: vec![Tex(0, 0)], ..Mat::new("m") }],
            &[Pane::new("p")],
        ))
        .case("missing-texture"),
    );
    l.push(
        b(flat(&[small()]))
            .anim(START, slide(false))
            .frames(&[(0.0, 1.0), (30.0, 1.0), (60.0, 1.0), (600.0, 1.0), (17.5, 1.0), (17.5, 1.5), (-5.0, 1.0)])
            .case("slide-start"),
    );
    l.push(
        b(flat(&[small()]))
            .anim(LOOP, slide(true))
            .frames(&[(0.0, 1.0), (30.0, 1.0), (90.0, 1.0), (77.3, 1.0)])
            .case("slide-loop"),
    );
    let loop_shift = animation(
        60,
        true,
        &[Target {
            name: "p",
            material: false,
            tags: vec![Tag("RLPA", vec![track(0, 1, true, &[k(0.0, 0.0), k(60.0, 40.0)])])],
        }],
        &[],
    );
    l.push(
        b(flat(&[small()]))
            .anim(START, slide(false))
            .anim(LOOP, loop_shift)
            .frames(&[(0.0, 1.0), (45.0, 1.0), (60.0, 1.0), (90.0, 1.0), (150.0, 1.0)])
            .case("start-loop"),
    );
    l.push(
        b(flat(&[red_pane(100.0, 100.0)]))
            .anim(
                START,
                one(vec![Tag("RLVC", vec![track(0, 16, true, &[k(0.0, 0.0), k(60.0, 255.0)])])], "p", false, 60, &[]),
            )
            .frames(&[(0.0, 1.0), (30.0, 1.0), (60.0, 1.0)])
            .case("fade"),
    );
    l.push(
        b(flat(&[red_pane(100.0, 100.0)]))
            .anim(
                START,
                one(vec![Tag("RLVI", vec![track(0, 0, false, &[k(0.0, 0.0), k(10.0, 1.0)])])], "p", false, 20, &[]),
            )
            .frames(&[(5.0, 1.0), (9.9, 1.0), (10.0, 1.0)])
            .case("blink"),
    );
    let halves = tpl_opaque(4, 4, |x, _| if x < 2 { RED5 } else { GREEN5 });
    l.push(
        b(layout(
            200.0,
            100.0,
            &["h.tpl"],
            &[Mat { textures: vec![Tex(0, 1)], srt: vec![[0.0, 0.0, 0.0, 1.0, 1.0]], ..Mat::new("m") }],
            &[Pane::new("p")],
        ))
        .anim(START, one(vec![Tag("RLTS", vec![track(0, 0, true, &[k(0.0, 0.0), k(60.0, 0.5)])])], "m", true, 60, &[]))
        .tex("h.tpl", halves)
        .frames(&[(0.0, 1.0), (15.0, 1.0), (60.0, 1.0)])
        .case("scroll"),
    );
    l.push(
        b(layout(200.0, 100.0, &[], &[Mat { color: Some([255; 4]), ..Mat::new("m") }], &[Pane::new("p")]))
            .anim(
                START,
                one(
                    vec![Tag(
                        "RLMC",
                        vec![
                            track(0, 0, false, &[k(0.0, 255.0), k(5.0, 0.0)]),
                            track(0, 2, false, &[k(0.0, 255.0), k(5.0, 0.0)]),
                        ],
                    )],
                    "m",
                    true,
                    10,
                    &[],
                ),
            )
            .frames(&[(0.0, 1.0), (2.5, 1.0), (5.0, 1.0)])
            .case("tint"),
    );
    l.push(
        b(layout(
            200.0,
            100.0,
            &["a.tpl", "b.tpl"],
            &[Mat { textures: vec![Tex(0, 0)], ..Mat::new("m") }],
            &[Pane::new("p")],
        ))
        .anim(
            START,
            one(
                vec![Tag("RLTP", vec![track(0, 0, false, &[k(0.0, 0.0), k(5.0, 1.0)])])],
                "m",
                true,
                10,
                &["a.tpl", "b.tpl"],
            ),
        )
        .tex("a.tpl", tpl_opaque(4, 4, |_, _| RED5))
        .tex("b.tpl", tpl_opaque(4, 4, |_, _| BLUE5))
        .frames(&[(0.0, 1.0), (5.0, 1.0)])
        .case("swap"),
    );
    l.push(b(flat(&[red_pane(100.0, 100.0)])).frames(&[(0.0, 2.0)]).case("scale2"));
    l.push(
        b(layout(
            200.0,
            100.0,
            &["q.tpl"],
            &[Mat { textures: vec![Tex(0, 1)], srt: vec![[0.0, 0.0, 0.0, 1.0, 1.0]], ..Mat::new("m") }],
            &[Pane::new("p")],
        ))
        .anim(START, one(vec![Tag("RLTS", vec![track(0, 2, true, &[k(0.0, 0.0), k(60.0, 90.0)])])], "m", true, 60, &[]))
        .tex("q.tpl", quadrants())
        .frames(&[(0.0, 1.0), (30.0, 1.0), (60.0, 1.0), (20.0, 1.5)])
        .case("srt-rotate"),
    );
    l.push(
        b(layout(
            200.0,
            100.0,
            &["q.tpl"],
            &[Mat { textures: vec![Tex(0, 2)], srt: vec![[0.1, 0.0, 0.0, 2.0, 2.0]], ..Mat::new("m") }],
            &[Pane::new("p")],
        ))
        .tex("q.tpl", quadrants())
        .frames(&[(0.0, 1.0), (0.0, 1.6)])
        .case("mirror"),
    );
    l.push(
        b(layout(
            200.0,
            100.0,
            &["a.tpl", "b.tpl"],
            &[Mat { textures: vec![Tex(0, 0), Tex(1, 0)], ..Mat::new("m") }],
            &[Pane::new("p")],
        ))
        .tex("a.tpl", tpl_opaque(4, 4, |_, _| BLUE5))
        .tex("b.tpl", tpl_words(4, 4, |x, _| if x < 2 { 0x0000 } else { 0x4F00 }))
        .frames(&[(0.0, 1.0), (0.0, 2.0)])
        .case("layered"),
    );
    l.push(
        b(flat(&[Pane::new("p")]))
            .anim(
                START,
                one(
                    vec![Tag(
                        "RLVC",
                        vec![
                            track(0, 1, true, &[k(0.0, 255.0), k(60.0, 0.0)]),
                            track(0, 2, true, &[k(0.0, 255.0), k(60.0, 0.0)]),
                            track(0, 13, false, &[k(0.0, 255.0), k(30.0, 0.0)]),
                            track(0, 0, true, &[k(0.0, 255.0), k(60.0, 50.0)]),
                        ],
                    )],
                    "p",
                    false,
                    60,
                    &[],
                ),
            )
            .frames(&[(0.0, 1.0), (30.0, 1.0), (60.0, 1.0)])
            .case("vertex-anim"),
    );
    l.push(
        b(flat(&[Pane { w: 60.0, h: 30.0, colors: [RED, GREEN, BLUE, 0xFFFF_FFFF], ..Pane::new("p") }]))
            .anim(
                START,
                one(
                    vec![Tag(
                        "RLPA",
                        vec![
                            track(0, 8, true, &[Key(0.0, 60.0, 1.0), Key(20.0, 100.0, -2.0), Key(60.0, 40.0, 0.5)]),
                            track(0, 9, true, &[k(0.0, 30.0), Key(60.0, 70.0, 1.0)]),
                            track(0, 6, true, &[k(0.0, 1.0), k(60.0, 1.5)]),
                            track(0, 5, true, &[k(0.0, 0.0), k(60.0, 45.0)]),
                            track(0, 0, false, &[k(0.0, -30.0), k(30.0, 30.0)]),
                            track(0, 1, true, &[Key(0.0, 0.0, 0.7), Key(60.0, 20.0, -0.3)]),
                            track(0, 7, true, &[k(0.0, 1.0), k(60.0, 0.6)]),
                        ],
                    )],
                    "p",
                    false,
                    60,
                    &[],
                ),
            )
            .frames(&[(0.0, 1.0), (12.3, 1.0), (30.0, 1.0), (45.7, 1.37), (60.0, 1.0)])
            .case("size-anim"),
    );
    let c = Pane { x: 15.0, w: 40.0, h: 20.0, rot_z: 15.0, colors: [RED, GREEN, BLUE, YELLOW], ..Pane::new("c") };
    let d = Pane { x: -20.0, y: 10.0, w: 15.0, h: 15.0, influenced: false, colors: [0x8080_80FF; 4], ..Pane::new("d") };
    l.push(
        b(flat(&[Pane {
            kind: "pan1",
            x: 20.0,
            y: -10.0,
            rot_z: 30.0,
            scale: 1.5,
            alpha: 180,
            children: vec![c, d],
            ..Pane::new("g")
        }]))
        .frames(&[(0.0, 1.0), (0.0, 1.37)])
        .case("nested-rot"),
    );
    l.push(
        b(layout(
            200.0,
            100.0,
            &["q.tpl"],
            &[Mat { textures: vec![Tex(0, 0)], srt: vec![[0.3, 0.2, 0.0, 1.0, 1.0]], ..Mat::new("m") }],
            &[Pane::new("p")],
        ))
        .tex("q.tpl", quadrants())
        .case("clamp-offset"),
    );
    let alpha_tex = tpl_words(8, 8, |x, y| {
        let (x, y) = (x as u16, y as u16);
        if (x + y) % 3 == 0 {
            0x0000
        } else if x < 4 {
            0x5000 | ((x * 3) << 8) | ((y * 2) << 4) | 0xF
        } else {
            0x8000 | ((x * 4) << 10) | ((y * 4) << 5) | 9
        }
    });
    l.push(
        b(layout(
            200.0,
            100.0,
            &["t.tpl"],
            &[Mat { textures: vec![Tex(0, 0)], color: Some([200, 255, 128, 220]), ..Mat::new("m") }],
            &[Pane { alpha: 200, colors: [0xFF80_40FF, 0x40FF_80C0, 0x8040_FFFF, 0xFFFF_FF80], ..Pane::new("p") }],
        ))
        .tex("t.tpl", alpha_tex)
        .frames(&[(0.0, 1.0), (0.0, 1.73)])
        .case("alpha-texture"),
    );
    l.push(
        b(flat(&[
            Pane { w: 0.0, colors: [RED; 4], ..Pane::new("z") },
            Pane { scale: 0.0, colors: [RED; 4], ..Pane::new("s") },
            small(),
        ]))
        .case("zero-size"),
    );
    l.push(
        b(layout(200.0, 100.0, &[], &[], &[Pane { material: 7, colors: [RED; 4], ..Pane::new("p") }]))
            .case("no-material"),
    );
    l.push(
        b(flat(&[
            Pane { w: 20.0, h: 10.0, y: 20.0, colors: [RED; 4], ..Pane::new("p") },
            Pane { w: 20.0, h: 10.0, y: -20.0, colors: [GREEN; 4], ..Pane::new("p") },
        ]))
        .anim(START, slide(false))
        .frames(&[(0.0, 1.0), (30.0, 1.0)])
        .case("dup-names"),
    );
    // A 608 x 456 layout like a channel banner.
    let bg =
        Pane { w: 608.0, h: 456.0, colors: [0x2030_60FF, 0x2030_60FF, 0x6080_C0FF, 0x6080_C0FF], ..Pane::new("bg") };
    let dot = Pane {
        w: 30.0,
        h: 30.0,
        x: 100.0,
        y: -100.0,
        colors: [0xFFD0_00FF, 0xFFD0_00FF, 0xFF80_00FF, 0xFF80_00FF],
        ..Pane::new("dot")
    };
    let logo = Pane {
        w: 200.0,
        h: 200.0,
        y: 30.0,
        rot_z: 12.0,
        material: 1,
        colors: [0xFFFF_FFFF, 0xFFFF_FFFF, 0xFFFF_FFC0, 0xFFFF_FFC0],
        children: vec![dot],
        ..Pane::new("logo")
    };
    let big = layout(
        608.0,
        456.0,
        &["q.tpl"],
        &[Mat::new("m"), Mat { textures: vec![Tex(0, 0)], ..Mat::new("t") }],
        &[bg, logo, Pane { kind: "txt1", ..Pane::new("txt") }, Pane { kind: "wnd1", ..Pane::new("frame") }],
    );
    let big_start = animation(
        40,
        false,
        &[
            Target {
                name: "logo",
                material: false,
                tags: vec![
                    Tag("RLPA", vec![track(0, 1, true, &[k(0.0, -300.0), k(40.0, 30.0)])]),
                    Tag("RLVC", vec![track(0, 16, true, &[k(0.0, 0.0), k(40.0, 255.0)])]),
                ],
            },
            Target {
                name: "bg",
                material: false,
                tags: vec![Tag("RLVC", vec![track(0, 16, false, &[k(0.0, 128.0), k(10.0, 255.0)])])],
            },
        ],
        &[],
    );
    let big_loop = animation(
        120,
        true,
        &[Target {
            name: "dot",
            material: false,
            tags: vec![Tag(
                "RLPA",
                vec![
                    track(0, 1, true, &[k(0.0, -100.0), k(60.0, -80.0), k(120.0, -100.0)]),
                    track(0, 5, true, &[k(0.0, 0.0), k(120.0, 360.0)]),
                ],
            )],
        }],
        &[],
    );
    l.push(
        b(big)
            .anim(START, big_start)
            .anim(LOOP, big_loop)
            .tex("q.tpl", quadrants())
            .frames(&[(0.0, 1.0), (20.0, 1.0), (40.0, 1.0), (70.0, 1.0), (100.0, 0.5), (160.0, 1.0)])
            .case("banner608"),
    );
    l
}
