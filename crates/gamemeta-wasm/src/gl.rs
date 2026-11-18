// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The GL function table for WebGL 2: every entry point `banner3ds-gles` calls is an `extern "system"` function here that
//! forwards its arguments (as doubles) to `js/gl.js`, which acts on the context made current with [`set_context`]. The
//! renderer and its loader contract are unchanged: this is just a loader whose "addresses" are these functions.

use std::ffi::{CStr, c_void};

use banner3ds_gles::Gl;
use wasm_bindgen::prelude::*;

/// A GL argument as the double JS receives (GL enums, ints, floats, flags and wasm addresses all fit exactly).
trait Wire {
    fn wire(self) -> f64;
}
macro_rules! numbers { ($($t:ty),*) => {$(impl Wire for $t { fn wire(self) -> f64 { self as f64 } })*}; }
numbers!(i32, u32, f32, u8, isize);
impl<T> Wire for *const T {
    fn wire(self) -> f64 {
        self as usize as f64
    }
}
impl<T> Wire for *mut T {
    fn wire(self) -> f64 {
        self as usize as f64
    }
}

macro_rules! gl_shim {
    ($($field:ident = $name:literal : fn($($arg:ident: $ty:ty),*) $(-> $ret:ty)?;)*) => {
        mod js {
            use wasm_bindgen::prelude::*;
            #[wasm_bindgen(module = "/js/gl.js")]
            extern "C" {
                pub fn init(memory: JsValue);
                pub fn set_context(gl: &JsValue);
                $(pub fn $field($($arg: f64),*) -> f64;)*
            }
        }

        mod shim {
            use super::{Wire, js};
            use std::ffi::{c_char, c_void};
            $(
                #[allow(clippy::too_many_arguments, unused)]
                pub(super) unsafe extern "system" fn $field($($arg: $ty),*) $(-> $ret)? {
                    let _r = js::$field($($arg.wire()),*);
                    $(_r as $ret)?
                }
            )*
        }

        /// The entry point called `name` (a GL function name), null for any other.
        pub fn lookup(name: &CStr) -> *const c_void {
            $(if name == $name { return shim::$field as *const c_void; })*
            std::ptr::null()
        }
    };
}

gl_shim! {
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

/// The renderer's GL table over WebGL 2 (every entry point is found: the table is `banner3ds-gles`'s own list).
pub fn table() -> Gl {
    // SAFETY: each address is the forwarding function of the GL name asked for (generated from the renderer's own table), and
    // the context behind it is whichever `set_context` made current, which the callers keep current around every GL call.
    unsafe { Gl::load(lookup) }.expect("every GL entry point the renderer uses has a shim")
}

/// Makes `gl` (a `WebGL2RenderingContext`) the context every GL call acts on until the next call.
pub fn set_context(gl: &JsValue) {
    js::init(wasm_bindgen::memory());
    js::set_context(gl);
}
