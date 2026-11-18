// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The OpenGL ES 3.0 calls the renderer makes, and nothing else, as a table of entry points the caller's loader fills
//! (`eglGetProcAddress`, `SDL_GL_GetProcAddress`, a WebGL 2 shim: anything that maps a GL name to a function). Every call acts
//! on the context current on the calling thread; this crate makes no context, no window and no thread.

use core::ffi::{CStr, c_char, c_void};

/// GL enum values the renderer uses (the same on every GLES 3, WebGL 2 and desktop GL implementation).
#[allow(missing_docs)]
pub mod consts {
    pub const DEPTH_BUFFER_BIT: u32 = 0x100;
    pub const COLOR_BUFFER_BIT: u32 = 0x4000;
    pub const TRIANGLES: u32 = 0x4;
    pub const CULL_FACE: u32 = 0xB44;
    pub const DEPTH_TEST: u32 = 0xB71;
    pub const BLEND: u32 = 0xBE2;
    pub const SCISSOR_TEST: u32 = 0xC11;
    pub const FRONT: u32 = 0x404;
    pub const BACK: u32 = 0x405;
    pub const LEQUAL: u32 = 0x203;
    pub const ARRAY_BUFFER: u32 = 0x8892;
    pub const ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
    pub const STATIC_DRAW: u32 = 0x88E4;
    pub const DYNAMIC_DRAW: u32 = 0x88E8;
    pub const VERTEX_SHADER: u32 = 0x8B31;
    pub const FRAGMENT_SHADER: u32 = 0x8B30;
    pub const COMPILE_STATUS: u32 = 0x8B81;
    pub const LINK_STATUS: u32 = 0x8B82;
    pub const INFO_LOG_LENGTH: u32 = 0x8B84;
    pub const TEXTURE_2D: u32 = 0xDE1;
    pub const TEXTURE0: u32 = 0x84C0;
    pub const TEXTURE_MAG_FILTER: u32 = 0x2800;
    pub const TEXTURE_MIN_FILTER: u32 = 0x2801;
    pub const TEXTURE_WRAP_S: u32 = 0x2802;
    pub const TEXTURE_WRAP_T: u32 = 0x2803;
    pub const NEAREST: u32 = 0x2600;
    pub const LINEAR: u32 = 0x2601;
    pub const REPEAT: u32 = 0x2901;
    pub const CLAMP_TO_EDGE: u32 = 0x812F;
    pub const MIRRORED_REPEAT: u32 = 0x8370;
    pub const RGBA: u32 = 0x1908;
    pub const RGBA8: u32 = 0x8058;
    pub const RGBA32F: u32 = 0x8814;
    pub const UNSIGNED_BYTE: u32 = 0x1401;
    pub const UNSIGNED_INT: u32 = 0x1405;
    pub const FLOAT: u32 = 0x1406;
    pub const UNPACK_ALIGNMENT: u32 = 0xCF5;
    pub const ZERO: u32 = 0;
    pub const SRC_ALPHA: u32 = 0x302;
    pub const FUNC_ADD: u32 = 0x8006;
}

/// An entry point the loader did not find.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissingFunction(pub &'static CStr);

macro_rules! gl_table {
    ($($field:ident = $name:literal : fn($($arg:ident: $ty:ty),*) $(-> $ret:ty)?;)*) => {
        /// The GL entry points (filled by [`Gl::load`]). Copy it freely; it holds only function addresses.
        #[derive(Clone, Copy)]
        pub struct Gl {
            $($field: unsafe extern "system" fn($($ty),*) $(-> $ret)?,)*
        }

        impl core::fmt::Debug for Gl {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("Gl { .. }")
            }
        }

        impl Gl {
            /// The names [`load`](Self::load) asks for.
            pub const NAMES: &'static [&'static CStr] = &[$($name),*];

            /// Every entry point from `load` (a GL name to its address, null when missing).
            ///
            /// # Safety
            /// Each non-null address `load` returns must be the GL ES 3.0 function of that name (with GL's calling
            /// convention), valid for the context the renderer will be used with.
            pub unsafe fn load(mut load: impl FnMut(&CStr) -> *const c_void) -> Result<Gl, MissingFunction> {
                Ok(Gl {
                    $($field: {
                        let p = load($name);
                        if p.is_null() {
                            return Err(MissingFunction($name));
                        }
                        // SAFETY: the caller promises `p` is this GL function.
                        unsafe { core::mem::transmute::<*const c_void, unsafe extern "system" fn($($ty),*) $(-> $ret)?>(p) }
                    },)*
                })
            }

            $(
                #[inline]
                #[allow(clippy::too_many_arguments)]
                pub(crate) fn $field(&self, $($arg: $ty),*) $(-> $ret)? {
                    // SAFETY: `load`'s contract makes this the GL function; the renderer's entry points (all `unsafe`) require
                    // the context it was loaded for to be current, and every pointer argument here points into live memory
                    // of the size GL reads.
                    unsafe { (self.$field)($($arg),*) }
                }
            )*
        }
    };
}

gl_table! {
    viewport = c"glViewport": fn(x: i32, y: i32, w: i32, h: i32);
    scissor = c"glScissor": fn(x: i32, y: i32, w: i32, h: i32);
    clear_color = c"glClearColor": fn(r: f32, g: f32, b: f32, a: f32);
    clear = c"glClear": fn(mask: u32);
    color_mask = c"glColorMask": fn(r: u8, g: u8, b: u8, a: u8);
    enable = c"glEnable": fn(cap: u32);
    disable = c"glDisable": fn(cap: u32);
    depth_mask = c"glDepthMask": fn(flag: u8);
    depth_func = c"glDepthFunc": fn(func: u32);
    cull_face = c"glCullFace": fn(mode: u32);
    blend_func_separate = c"glBlendFuncSeparate": fn(sr: u32, dr: u32, sa: u32, da: u32);
    blend_equation_separate = c"glBlendEquationSeparate": fn(rgb: u32, alpha: u32);
    create_shader = c"glCreateShader": fn(kind: u32) -> u32;
    shader_source = c"glShaderSource": fn(shader: u32, count: i32, strings: *const *const c_char, lengths: *const i32);
    compile_shader = c"glCompileShader": fn(shader: u32);
    get_shaderiv = c"glGetShaderiv": fn(shader: u32, pname: u32, out: *mut i32);
    get_shader_info_log = c"glGetShaderInfoLog": fn(shader: u32, size: i32, len: *mut i32, log: *mut c_char);
    delete_shader = c"glDeleteShader": fn(shader: u32);
    create_program = c"glCreateProgram": fn() -> u32;
    attach_shader = c"glAttachShader": fn(program: u32, shader: u32);
    bind_attrib_location = c"glBindAttribLocation": fn(program: u32, index: u32, name: *const c_char);
    link_program = c"glLinkProgram": fn(program: u32);
    get_programiv = c"glGetProgramiv": fn(program: u32, pname: u32, out: *mut i32);
    get_program_info_log = c"glGetProgramInfoLog": fn(program: u32, size: i32, len: *mut i32, log: *mut c_char);
    delete_program = c"glDeleteProgram": fn(program: u32);
    use_program = c"glUseProgram": fn(program: u32);
    get_uniform_location = c"glGetUniformLocation": fn(program: u32, name: *const c_char) -> i32;
    uniform1i = c"glUniform1i": fn(location: i32, value: i32);
    uniform1fv = c"glUniform1fv": fn(location: i32, count: i32, value: *const f32);
    uniform2fv = c"glUniform2fv": fn(location: i32, count: i32, value: *const f32);
    uniform3fv = c"glUniform3fv": fn(location: i32, count: i32, value: *const f32);
    uniform4fv = c"glUniform4fv": fn(location: i32, count: i32, value: *const f32);
    uniform_matrix3fv = c"glUniformMatrix3fv": fn(location: i32, count: i32, transpose: u8, value: *const f32);
    uniform_matrix4fv = c"glUniformMatrix4fv": fn(location: i32, count: i32, transpose: u8, value: *const f32);
    gen_vertex_arrays = c"glGenVertexArrays": fn(n: i32, out: *mut u32);
    gen_buffers = c"glGenBuffers": fn(n: i32, out: *mut u32);
    gen_textures = c"glGenTextures": fn(n: i32, out: *mut u32);
    delete_vertex_arrays = c"glDeleteVertexArrays": fn(n: i32, ids: *const u32);
    delete_buffers = c"glDeleteBuffers": fn(n: i32, ids: *const u32);
    delete_textures = c"glDeleteTextures": fn(n: i32, ids: *const u32);
    bind_vertex_array = c"glBindVertexArray": fn(id: u32);
    bind_buffer = c"glBindBuffer": fn(target: u32, id: u32);
    buffer_data = c"glBufferData": fn(target: u32, size: isize, data: *const c_void, usage: u32);
    buffer_sub_data = c"glBufferSubData": fn(target: u32, offset: isize, size: isize, data: *const c_void);
    enable_vertex_attrib_array = c"glEnableVertexAttribArray": fn(index: u32);
    vertex_attrib_pointer = c"glVertexAttribPointer": fn(index: u32, size: i32, kind: u32, normalized: u8, stride: i32, offset: *const c_void);
    draw_elements = c"glDrawElements": fn(mode: u32, count: i32, kind: u32, offset: *const c_void);
    active_texture = c"glActiveTexture": fn(unit: u32);
    bind_texture = c"glBindTexture": fn(target: u32, id: u32);
    pixel_storei = c"glPixelStorei": fn(pname: u32, value: i32);
    tex_image_2d = c"glTexImage2D": fn(target: u32, level: i32, internal: i32, w: i32, h: i32, border: i32, format: u32, kind: u32, data: *const c_void);
    tex_parameteri = c"glTexParameteri": fn(target: u32, pname: u32, value: i32);
}

impl Gl {
    pub(crate) fn new_name(&self, f: fn(&Gl, i32, *mut u32)) -> u32 {
        let mut id = 0u32;
        f(self, 1, &mut id);
        id
    }

    pub(crate) fn delete(&self, f: fn(&Gl, i32, *const u32), id: u32) {
        if id != 0 {
            f(self, 1, &id);
        }
    }
}
