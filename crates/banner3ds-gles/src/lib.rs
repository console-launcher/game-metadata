// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! An OpenGL ES 3.0 / WebGL 2 renderer for 3DS HOME banners: a [`BannerScene`] drawn into whatever framebuffer the caller has
//! bound, through a table of GL entry points the caller's loader fills ([`Gl::load`]). No windowing or context crate, no
//! threads: the caller makes the context current, then [`Renderer::new`] creates the scene's programs, buffers and textures,
//! [`Renderer::draw`] draws a frame into a viewport, and [`Renderer::destroy`] deletes them.
//!
//! It draws what `banner3ds-soft` draws (the pixel reference): the same framing, turntable, clips, draw order, culling, depth,
//! blend, combiners and fragment lighting, with every vertex posed, skinned and placed in world space on the CPU by the code
//! the soft renderer uses (`banner3ds_scene::draw`). The shaders keep to GLSL ES 3.00 inside WebGL 2: clamp to border is read
//! off the border colour in the shader, the lighting tables are an RGBA32F texture read with `texelFetch`, and the PICA blend
//! constant factors read one as in the soft renderer. Where GL rasterises differently (edge rules, derivatives for bump
//! tangents, filtering across a repeat seam, the 0.002 tolerance of the equal and not-equal alpha tests) the pixels differ
//! slightly; the tests hold them to the soft renderer within a stated tolerance.
#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]

extern crate alloc;

mod gl;
pub mod glsl;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::ffi::c_void;

use banner3ds_scene::coords::SPHERE;
use banner3ds_scene::draw::math::{self, Mat34};
use banner3ds_scene::draw::{Camera, Player, Pose, billboard, camera, light, order, skin};
use banner3ds_scene::lighting::MAX_LIGHTS;
use banner3ds_scene::{BannerMaterial, BannerMesh, BannerScene};

use gl::consts as c;
pub use gl::{Gl, MissingFunction, consts};
use glsl::LutAtlas;

/// position 3, uv0 2, uv1 2, uv2 2, colour 4, normal 3.
const STRIDE: usize = 16;
const ATTRIBUTES: [(&core::ffi::CStr, i32); 6] =
    [(c"aPos", 3), (c"aUv0", 2), (c"aUv1", 2), (c"aUv2", 2), (c"aColor", 4), (c"aNormal", 3)];

/// Where to draw in the bound framebuffer, in GL window coordinates (x, y of the bottom-left corner, y up).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    /// Left edge.
    pub x: i32,
    /// Bottom edge.
    pub y: i32,
    /// Width in pixels.
    pub width: i32,
    /// Height in pixels.
    pub height: i32,
}

impl Viewport {
    /// The `width` x `height` region at the origin.
    pub fn of(width: i32, height: i32) -> Viewport {
        Viewport { x: 0, y: 0, width, height }
    }
}

/// How to draw a frame; the default is HOME's view.
#[derive(Debug, Clone, Default)]
pub struct DrawOptions {
    /// The camera; `None` for the banner's framing (HOME's fixed view).
    pub camera: Option<Camera>,
    /// The clips' frame when it differs from the turntable's; `None` for the same instant.
    pub clip_frame: Option<f32>,
}

/// Why a renderer could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// No mesh of the scene could be set up (none drawable, or every program failed: see the log).
    NothingDrawable(Vec<String>),
}

struct Program {
    id: u32,
    mvp: i32,
    scale: i32,
    rot: i32,
    trans: i32,
    constants: i32,
    buffer: i32,
    tex: [i32; 3],
    view: i32,
    eye: i32,
    turn: i32,
    lut: i32,
    global_ambient: i32,
    light_ambient: i32,
    light_diffuse: i32,
    light_specular0: i32,
    light_specular1: i32,
    window: i32,
}

struct GpuMesh {
    model: usize,
    mesh: usize,
    program: usize,
    vao: u32,
    vbo: u32,
    ibo: u32,
    index_count: i32,
    data: Vec<f32>,
    bound: [bool; 3],
}

/// A scene's GL resources in one context, ready to draw frames.
pub struct Renderer<'a> {
    scene: &'a BannerScene,
    gl: Gl,
    player: Player<'a>,
    atlas: LutAtlas,
    textures: BTreeMap<String, u32>,
    programs: Vec<Program>,
    sources: BTreeMap<String, Option<usize>>,
    meshes: Vec<GpuMesh>,
    by_mesh: BTreeMap<(usize, usize), usize>,
    lut_texture: u32,
    window: Option<(usize, u32, u32, u32)>,
    log: Vec<String>,
}

impl core::fmt::Debug for Renderer<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Renderer").field("meshes", &self.meshes.len()).field("programs", &self.programs.len()).finish()
    }
}

impl<'a> Renderer<'a> {
    /// Compiles the scene's programs and uploads its meshes and textures into the current context.
    ///
    /// # Safety
    /// The context `gl` was loaded for must be current on this thread (for this call and every later one on the renderer).
    pub unsafe fn new(scene: &'a BannerScene, gl: Gl) -> Result<Renderer<'a>, Error> {
        let mut r = Renderer {
            scene,
            gl,
            player: Player::new(scene),
            atlas: LutAtlas::of(scene),
            textures: BTreeMap::new(),
            programs: Vec::new(),
            sources: BTreeMap::new(),
            meshes: Vec::new(),
            by_mesh: BTreeMap::new(),
            lut_texture: 0,
            window: None,
            log: Vec::new(),
        };
        for (mi, model) in scene.models.iter().enumerate() {
            for (ni, mesh) in model.meshes.iter().enumerate() {
                if mesh.indices.len() < 3 || mesh.material.face_culling == 2 || !skin::consistent(mesh) {
                    continue;
                }
                if let Some(g) = r.upload(mi, ni, mesh) {
                    r.by_mesh.insert((mi, ni), r.meshes.len());
                    r.meshes.push(g);
                }
            }
        }
        if r.meshes.is_empty() {
            let log = core::mem::take(&mut r.log);
            // SAFETY: the caller's context is current.
            unsafe { r.destroy() };
            return Err(Error::NothingDrawable(log));
        }
        Ok(r)
    }

    /// The loop length in frames (60 to the second); 0 when the banner does not animate.
    pub fn frames(&self) -> f32 {
        self.player.frames
    }

    /// Whether anything moves.
    pub fn animated(&self) -> bool {
        self.player.animated
    }

    /// The player that poses the scene.
    pub fn player(&self) -> &Player<'a> {
        &self.player
    }

    /// Shader compile and link failures (each such mesh is left out).
    pub fn log(&self) -> &[String] {
        &self.log
    }

    fn upload(&mut self, mi: usize, ni: usize, mesh: &BannerMesh) -> Option<GpuMesh> {
        let gl = self.gl;
        let material = &*mesh.material;
        let v = &*mesh.vertices;
        let normals = v.normals.is_some();
        let slot = |s: usize| material.slots.get(s).and_then(|x| x.as_ref());
        let bound: [bool; 3] = core::array::from_fn(|s| {
            slot(s).is_some_and(|sl| {
                let coordinates = (sl.mapping == SPHERE && normals) || v.uv0.is_some();
                coordinates
                    && self.scene.textures.get(&sl.texture).is_some_and(|t| valid_image(t.width, t.height, &t.pixels))
            })
        });
        let uv_sets: [i32; 3] = core::array::from_fn(|s| slot(s).map_or(0, |x| x.uv_set));
        let mappings: [i32; 3] =
            core::array::from_fn(|s| if glsl::sphere_slot(material, s, normals) { SPHERE } else { 0 });
        let modes: [i32; 3] = core::array::from_fn(|s| slot(s).map_or(0, |x| x.matrix_mode));
        let lit = material.fragment_lighting;
        let lights = self.scene.lights_for(material);
        let vertex = glsl::vertex(uv_sets, lit, mappings, modes);
        let fragment = glsl::fragment(material, bound, &lights, lit.then_some(&self.atlas), normals);
        let program = self.program(&vertex, &fragment)?;
        for (s, &on) in bound.iter().enumerate() {
            if let (true, Some(sl)) = (on, slot(s)) {
                self.texture(&sl.texture);
            }
        }
        let mut data = vec![0f32; v.count * STRIDE];
        for i in 0..v.count {
            let o = i * STRIDE;
            let uv = |set: &Option<Vec<f32>>| set.as_ref().or(v.uv0.as_ref()).map(|u| [u[i * 2], u[i * 2 + 1]]);
            if let Some(u) = v.uv0.as_ref().map(|u| [u[i * 2], u[i * 2 + 1]]) {
                data[o + 3..o + 5].copy_from_slice(&u);
            }
            if let Some(u) = uv(&v.uv1) {
                data[o + 5..o + 7].copy_from_slice(&u);
            }
            if let Some(u) = uv(&v.uv2) {
                data[o + 7..o + 9].copy_from_slice(&u);
            }
            for k in 0..4 {
                data[o + 9 + k] = v.colors.as_ref().map_or(1.0, |c| c[i * 4 + k]);
            }
        }
        let vao = gl.new_name(Gl::gen_vertex_arrays);
        let vbo = gl.new_name(Gl::gen_buffers);
        let ibo = gl.new_name(Gl::gen_buffers);
        gl.bind_vertex_array(vao);
        gl.bind_buffer(c::ARRAY_BUFFER, vbo);
        gl.buffer_data(c::ARRAY_BUFFER, (data.len() * 4) as isize, data.as_ptr().cast(), c::DYNAMIC_DRAW);
        gl.bind_buffer(c::ELEMENT_ARRAY_BUFFER, ibo);
        let count = mesh.indices.len() / 3 * 3;
        gl.buffer_data(c::ELEMENT_ARRAY_BUFFER, (count * 4) as isize, mesh.indices.as_ptr().cast(), c::STATIC_DRAW);
        let mut offset = 0;
        for (a, (_, size)) in ATTRIBUTES.iter().enumerate() {
            gl.enable_vertex_attrib_array(a as u32);
            gl.vertex_attrib_pointer(a as u32, *size, c::FLOAT, 0, (STRIDE * 4) as i32, (offset * 4) as *const c_void);
            offset += *size as usize;
        }
        gl.bind_vertex_array(0);
        Some(GpuMesh { model: mi, mesh: ni, program, vao, vbo, ibo, index_count: count as i32, data, bound })
    }

    fn program(&mut self, vertex: &str, fragment: &str) -> Option<usize> {
        let key = alloc::format!("{vertex}\0{fragment}");
        if let Some(p) = self.sources.get(&key) {
            return *p;
        }
        let p = self.link(vertex, fragment);
        self.sources.insert(key, p);
        p
    }

    fn link(&mut self, vertex: &str, fragment: &str) -> Option<usize> {
        let gl = self.gl;
        let vs = self.compile(c::VERTEX_SHADER, vertex)?;
        let Some(fs) = self.compile(c::FRAGMENT_SHADER, fragment) else {
            gl.delete_shader(vs);
            return None;
        };
        let id = gl.create_program();
        gl.attach_shader(id, vs);
        gl.attach_shader(id, fs);
        for (i, (name, _)) in ATTRIBUTES.iter().enumerate() {
            gl.bind_attrib_location(id, i as u32, name.as_ptr());
        }
        gl.link_program(id);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        let mut ok = 0;
        gl.get_programiv(id, c::LINK_STATUS, &mut ok);
        if ok == 0 {
            let mut len = 0;
            gl.get_programiv(id, c::INFO_LOG_LENGTH, &mut len);
            let log = info_log(len, |size, out_len, buf| gl.get_program_info_log(id, size, out_len, buf));
            self.log.push(alloc::format!("banner program link failed: {log}"));
            gl.delete_program(id);
            return None;
        }
        let at = |name: &str| {
            let name = alloc::format!("{name}\0");
            gl.get_uniform_location(id, name.as_ptr().cast())
        };
        self.programs.push(Program {
            id,
            mvp: at("uMvp"),
            scale: at("uScale"),
            rot: at("uRot"),
            trans: at("uTrans"),
            constants: at("uConst"),
            buffer: at("uBuffer"),
            tex: [at("uTex0"), at("uTex1"), at("uTex2")],
            view: at("uView"),
            eye: at("uEye"),
            turn: at("uTurn"),
            lut: at("uLut"),
            global_ambient: at("uGAmb"),
            light_ambient: at("uLAmb"),
            light_diffuse: at("uLDif"),
            light_specular0: at("uLS0"),
            light_specular1: at("uLS1"),
            window: at("uWin"),
        });
        Some(self.programs.len() - 1)
    }

    fn compile(&mut self, kind: u32, source: &str) -> Option<u32> {
        let gl = self.gl;
        let id = gl.create_shader(kind);
        let ptr = source.as_ptr().cast();
        let len = source.len() as i32;
        gl.shader_source(id, 1, &ptr, &len);
        gl.compile_shader(id);
        let mut ok = 0;
        gl.get_shaderiv(id, c::COMPILE_STATUS, &mut ok);
        if ok == 0 {
            let mut len = 0;
            gl.get_shaderiv(id, c::INFO_LOG_LENGTH, &mut len);
            let log = info_log(len, |size, out_len, buf| gl.get_shader_info_log(id, size, out_len, buf));
            self.log.push(alloc::format!("banner shader compile failed: {log}"));
            gl.delete_shader(id);
            return None;
        }
        Some(id)
    }

    fn texture(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.textures.get(name) {
            return id;
        }
        let gl = self.gl;
        let image = &self.scene.textures[name];
        let id = gl.new_name(Gl::gen_textures);
        gl.bind_texture(c::TEXTURE_2D, id);
        gl.pixel_storei(c::UNPACK_ALIGNMENT, 4);
        gl.tex_image_2d(
            c::TEXTURE_2D,
            0,
            c::RGBA8 as i32,
            image.width as i32,
            image.height as i32,
            0,
            c::RGBA,
            c::UNSIGNED_BYTE,
            image.pixels.as_ptr().cast(),
        );
        gl.tex_parameteri(c::TEXTURE_2D, c::TEXTURE_MIN_FILTER, c::NEAREST as i32);
        gl.tex_parameteri(c::TEXTURE_2D, c::TEXTURE_MAG_FILTER, c::NEAREST as i32);
        self.textures.insert(name.into(), id);
        id
    }

    /// The lighting tables as a 256 x rows RGBA32F texture (read with `texelFetch` only: float textures don't filter in GLES 3).
    fn lut_texture(&mut self) -> u32 {
        if self.lut_texture == 0 && self.atlas.rows() > 0 {
            let gl = self.gl;
            let id = gl.new_name(Gl::gen_textures);
            gl.bind_texture(c::TEXTURE_2D, id);
            gl.tex_image_2d(
                c::TEXTURE_2D,
                0,
                c::RGBA32F as i32,
                256,
                self.atlas.rows() as i32,
                0,
                c::RGBA,
                c::FLOAT,
                self.atlas.data.as_ptr().cast(),
            );
            gl.tex_parameteri(c::TEXTURE_2D, c::TEXTURE_MIN_FILTER, c::NEAREST as i32);
            gl.tex_parameteri(c::TEXTURE_2D, c::TEXTURE_MAG_FILTER, c::NEAREST as i32);
            self.lut_texture = id;
        }
        self.lut_texture
    }

    /// Draws the banner at `frame` (60 to the second, looped by the clips) through HOME's view into `viewport` of the bound
    /// framebuffer, cleared to transparent black first (only inside the viewport).
    ///
    /// # Safety
    /// As for [`new`](Self::new): the renderer's context is current on this thread.
    pub unsafe fn draw(&mut self, frame: f32, viewport: Viewport) {
        // SAFETY: as the caller promised.
        unsafe { self.draw_with(frame, viewport, &DrawOptions::default()) }
    }

    /// [`draw`](Self::draw) with `options`.
    ///
    /// # Safety
    /// As for [`new`](Self::new).
    pub unsafe fn draw_with(&mut self, frame: f32, viewport: Viewport, options: &DrawOptions) {
        let pose = self.player.pose(frame, options.clip_frame.unwrap_or(frame));
        // SAFETY: as the caller promised.
        unsafe { self.draw_pose(&pose, viewport, options.camera.as_ref()) }
    }

    /// One frame of `pose` (from [`Player::pose`]) through `camera` (`None`: HOME's view) into `viewport`.
    ///
    /// # Safety
    /// As for [`new`](Self::new).
    pub unsafe fn draw_pose(&mut self, pose: &Pose, viewport: Viewport, camera: Option<&Camera>) {
        let gl = self.gl;
        let Viewport { x, y, width, height } = viewport;
        if width <= 0 || height <= 0 {
            return;
        }
        gl.color_mask(1, 1, 1, 1);
        gl.depth_mask(1);
        gl.enable(c::SCISSOR_TEST);
        gl.scissor(x, y, width, height);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(c::COLOR_BUFFER_BIT | c::DEPTH_BUFFER_BIT);
        gl.disable(c::SCISSOR_TEST);
        let framing;
        let cam = match camera {
            Some(c) => c,
            None => {
                framing = camera::framing(self.scene, width as f32 / height as f32);
                &framing
            }
        };
        // A file camera draws letterboxed at its own aspect (the clear covers the bars); GL counts y from the bottom.
        let port = camera::viewport(cam, width, height);
        gl.viewport(x + port[0], y + height - port[1] - port[3], port[2], port[3]);
        let view_proj = math::multiply(&cam.projection(port[2] as f32 / port[3] as f32), &cam.view());
        for d in order::of(self.scene, &self.player, cam, pose) {
            if let Some(&g) = self.by_mesh.get(&(d.model, d.mesh)) {
                self.draw_mesh(g, pose, &view_proj, cam);
            }
        }
        if let Some(e) = camera::home_window(cam, &port) {
            self.draw_window([x as f32 + e[0], (y + height) as f32 - e[1], e[2], e[3]]);
        }
        gl.bind_vertex_array(0);
        gl.use_program(0);
        gl.disable(c::BLEND);
        gl.depth_mask(1);
    }

    fn draw_mesh(&mut self, g: usize, pose: &Pose, view_proj: &[f32; 16], cam: &Camera) {
        let gl = self.gl;
        let scene = self.scene;
        let (mi, ni) = (self.meshes[g].model, self.meshes[g].mesh);
        let model = &scene.models[mi];
        let mesh = &model.meshes[ni];
        let mat = &*mesh.material;
        let v = &*mesh.vertices;
        let rest = self.player.rest(mi);
        // The soft renderer's posing: the pose's bones, turned toward the camera where billboarded, skinned, then placed.
        let posed = pose.bones_of(model, rest);
        let billboarded = billboard::apply(model, posed, cam);
        let bones: &[Mat34] = billboarded.as_deref().unwrap_or(posed);
        let skin_owned = billboarded.as_ref().map(|b| skin::skin(rest, b));
        let skinning: Option<&[Mat34]> = match &skin_owned {
            Some(s) => Some(s),
            None => pose.skinning.get(&model.name).map(|s| s.as_slice()),
        };
        let vertex_lit = mat.vertex_lighting || mat.hemisphere_lighting;
        let vertex_lights = vertex_lit.then(|| pose.vertex_lights_of(scene.vertex_lights_for(mat)));
        let hemisphere = scene.hemisphere_for(mat);
        {
            let data = &mut self.meshes[g].data;
            for i in 0..v.count {
                let o = i * STRIDE;
                let p = skin::position(v, mesh, skinning, Some(bones), i);
                let p = math::transform_point(&model.world, p[0], p[1], p[2]);
                data[o..o + 3].copy_from_slice(&p);
                if v.normals.is_some() || vertex_lit {
                    let n = skin::to_world_normal(&model.world, &skin::normal(v, mesh, skinning, Some(bones), i));
                    data[o + 13..o + 16].copy_from_slice(&n);
                    if let Some(lights) = &vertex_lights {
                        let vc = v.colors.as_ref().map(|c| &c[i * 4..i * 4 + 4]);
                        data[o + 9..o + 13]
                            .copy_from_slice(&light::vertex_primary(mat, lights, hemisphere, &n, &p, vc));
                    }
                }
            }
        }
        let gm = &self.meshes[g];
        gl.bind_buffer(c::ARRAY_BUFFER, gm.vbo);
        gl.buffer_sub_data(c::ARRAY_BUFFER, 0, (gm.data.len() * 4) as isize, gm.data.as_ptr().cast());
        let (pi, vao, index_count, bound) = (gm.program, gm.vao, gm.index_count, gm.bound);
        let p = &self.programs[pi];
        let (pid, mvp, view_loc, constants_loc, buffer_loc) = (p.id, p.mvp, p.view, p.constants, p.buffer);
        gl.use_program(pid);
        gl.uniform_matrix4fv(mvp, 1, 0, view_proj.as_ptr());
        if view_loc >= 0 {
            gl.uniform_matrix4fv(view_loc, 1, 0, cam.view().as_ptr());
        }
        if mat.fragment_lighting {
            self.bind_lighting(pi, pose, mat, &cam.eye);
        }
        self.bind_state(mat);
        self.bind_slots(pi, bound, pose, mat);
        let stages = pose.stages_of(mat);
        let mut constants = [0f32; 24];
        for (i, st) in stages.iter().take(6).enumerate() {
            constants[i * 4..i * 4 + 4].copy_from_slice(&st.constant);
        }
        gl.uniform4fv(constants_loc, 6, constants.as_ptr());
        gl.uniform4fv(buffer_loc, 1, mat.buffer_color.as_ptr());
        gl.bind_vertex_array(vao);
        gl.draw_elements(c::TRIANGLES, index_count, c::UNSIGNED_INT, core::ptr::null());
    }

    fn bind_lighting(&mut self, pi: usize, pose: &Pose, mat: &BannerMaterial, eye: &[f32; 3]) {
        let gl = self.gl;
        let lights: Vec<_> = self.scene.lights_for(mat).into_iter().take(MAX_LIGHTS).cloned().collect();
        let terms = light::terms(&pose.colors_of(mat), &lights, &self.scene.scene_ambient);
        let n = lights.len().max(1);
        let flat = |a: &[[f32; 3]]| {
            let mut out = vec![0f32; n * 3];
            for (i, c) in a.iter().take(n).enumerate() {
                out[i * 3..i * 3 + 3].copy_from_slice(c);
            }
            out
        };
        let lut = self.lut_texture();
        let p = &self.programs[pi];
        gl.uniform3fv(p.eye, 1, eye.as_ptr());
        gl.uniform_matrix3fv(p.turn, 1, 0, pose.turn_matrix().as_ptr());
        gl.uniform3fv(p.global_ambient, 1, terms.global_ambient.as_ptr());
        gl.uniform3fv(p.light_ambient, n as i32, flat(&terms.ambient).as_ptr());
        gl.uniform3fv(p.light_diffuse, n as i32, flat(&terms.diffuse).as_ptr());
        gl.uniform3fv(p.light_specular0, n as i32, flat(&terms.specular0).as_ptr());
        gl.uniform3fv(p.light_specular1, n as i32, flat(&terms.specular1).as_ptr());
        gl.active_texture(c::TEXTURE0 + 3);
        gl.bind_texture(c::TEXTURE_2D, lut);
        gl.uniform1i(p.lut, 3);
    }

    fn bind_state(&self, mat: &BannerMaterial) {
        let gl = self.gl;
        match mat.face_culling {
            0 => {
                gl.enable(c::CULL_FACE);
                gl.cull_face(c::FRONT);
            }
            1 => {
                gl.enable(c::CULL_FACE);
                gl.cull_face(c::BACK);
            }
            _ => gl.disable(c::CULL_FACE),
        }
        if mat.depth_test {
            gl.enable(c::DEPTH_TEST);
            gl.depth_func(c::LEQUAL);
        } else {
            gl.disable(c::DEPTH_TEST);
        }
        gl.depth_mask(u8::from(mat.depth_write && mat.blend_mode == 0));
        if mat.blend_mode == 0 {
            gl.disable(c::BLEND);
        } else {
            let f = glsl::blend_factors(mat.blend_function);
            gl.enable(c::BLEND);
            gl.blend_func_separate(f[0], f[1], f[2], f[3]);
            let e = glsl::blend_equations(mat.blend_function);
            gl.blend_equation_separate(e[0], e[1]);
        }
    }

    fn bind_slots(&mut self, pi: usize, bound: [bool; 3], pose: &Pose, mat: &BannerMaterial) {
        let gl = self.gl;
        let mut scale = [1f32; 6];
        let mut rot = [0f32; 3];
        let mut trans = [0f32; 6];
        for s in 0..3 {
            let slot = pose.slot_of(mat, s);
            if let Some(sl) = &slot {
                scale[s * 2..s * 2 + 2].copy_from_slice(&sl.scale);
                rot[s] = sl.rotation;
                trans[s * 2..s * 2 + 2].copy_from_slice(&sl.translation);
            }
            gl.uniform1i(self.programs[pi].tex[s], s as i32);
            if let (true, Some(sl)) = (bound[s], &slot) {
                let id = self.texture(&sl.texture);
                gl.active_texture(c::TEXTURE0 + s as u32);
                gl.bind_texture(c::TEXTURE_2D, id);
                let filter = if sl.linear { c::LINEAR } else { c::NEAREST } as i32;
                gl.tex_parameteri(c::TEXTURE_2D, c::TEXTURE_MIN_FILTER, filter);
                gl.tex_parameteri(c::TEXTURE_2D, c::TEXTURE_MAG_FILTER, filter);
                gl.tex_parameteri(c::TEXTURE_2D, c::TEXTURE_WRAP_S, wrap(sl.wrap_u));
                gl.tex_parameteri(c::TEXTURE_2D, c::TEXTURE_WRAP_T, wrap(sl.wrap_v));
            }
        }
        let p = &self.programs[pi];
        gl.uniform2fv(p.scale, 3, scale.as_ptr());
        gl.uniform1fv(p.rot, 3, rot.as_ptr());
        gl.uniform2fv(p.trans, 3, trans.as_ptr());
    }

    /// HOME's window: keeps the window's share of every pixel, so outside it the surface is as cleared (transparent).
    fn draw_window(&mut self, win: [f32; 4]) {
        let gl = self.gl;
        if self.window.is_none() {
            let Some(pi) = self.program(glsl::WINDOW_VERTEX, glsl::WINDOW_FRAGMENT) else { return };
            let quad: [f32; 12] = [-1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 1.0, 1.0, 0.0, -1.0, 1.0, 0.0];
            let index: [u32; 6] = [0, 1, 2, 0, 2, 3];
            let vao = gl.new_name(Gl::gen_vertex_arrays);
            let vbo = gl.new_name(Gl::gen_buffers);
            let ibo = gl.new_name(Gl::gen_buffers);
            gl.bind_vertex_array(vao);
            gl.bind_buffer(c::ARRAY_BUFFER, vbo);
            gl.buffer_data(c::ARRAY_BUFFER, 48, quad.as_ptr().cast(), c::STATIC_DRAW);
            gl.bind_buffer(c::ELEMENT_ARRAY_BUFFER, ibo);
            gl.buffer_data(c::ELEMENT_ARRAY_BUFFER, 24, index.as_ptr().cast(), c::STATIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer(0, 3, c::FLOAT, 0, 12, core::ptr::null());
            self.window = Some((pi, vao, vbo, ibo));
        }
        let Some((pi, vao, _, _)) = self.window else { return };
        let p = &self.programs[pi];
        gl.use_program(p.id);
        gl.uniform4fv(p.window, 1, win.as_ptr());
        gl.disable(c::DEPTH_TEST);
        gl.disable(c::CULL_FACE);
        gl.depth_mask(0);
        gl.enable(c::BLEND);
        gl.blend_func_separate(c::ZERO, c::SRC_ALPHA, c::ZERO, c::SRC_ALPHA);
        gl.blend_equation_separate(c::FUNC_ADD, c::FUNC_ADD);
        gl.bind_vertex_array(vao);
        gl.draw_elements(c::TRIANGLES, 6, c::UNSIGNED_INT, core::ptr::null());
    }

    /// Deletes every program, buffer and texture the renderer made. (Dropping a renderer without it leaves them to the
    /// context.)
    ///
    /// # Safety
    /// As for [`new`](Self::new).
    pub unsafe fn destroy(mut self) {
        let gl = self.gl;
        for g in self.meshes.drain(..) {
            gl.delete(Gl::delete_vertex_arrays, g.vao);
            gl.delete(Gl::delete_buffers, g.vbo);
            gl.delete(Gl::delete_buffers, g.ibo);
        }
        for p in self.programs.drain(..) {
            gl.delete_program(p.id);
        }
        for (_, t) in core::mem::take(&mut self.textures) {
            gl.delete(Gl::delete_textures, t);
        }
        gl.delete(Gl::delete_textures, self.lut_texture);
        if let Some((_, vao, vbo, ibo)) = self.window.take() {
            gl.delete(Gl::delete_vertex_arrays, vao);
            gl.delete(Gl::delete_buffers, vbo);
            gl.delete(Gl::delete_buffers, ibo);
        }
    }
}

/// PICA wrap modes: 0 clamp to edge, 1 clamp to border (edge here; the fragment shader reads the border colour), 2 repeat,
/// 3 mirror.
fn wrap(mode: i32) -> i32 {
    (match mode {
        2 => c::REPEAT,
        3 => c::MIRRORED_REPEAT,
        _ => c::CLAMP_TO_EDGE,
    }) as i32
}

fn valid_image(width: u32, height: u32, pixels: &[u8]) -> bool {
    width > 0
        && height > 0
        && width <= 8192
        && height <= 8192
        && pixels.len() as u64 == u64::from(width) * u64::from(height) * 4
}

fn info_log(len: i32, read: impl FnOnce(i32, *mut i32, *mut core::ffi::c_char)) -> String {
    let len = len.clamp(1, 1 << 16);
    let mut buf = vec![0u8; len as usize];
    let mut written = 0i32;
    read(len, &mut written, buf.as_mut_ptr().cast());
    buf.truncate(written.clamp(0, len) as usize);
    String::from_utf8_lossy(&buf).into_owned()
}
