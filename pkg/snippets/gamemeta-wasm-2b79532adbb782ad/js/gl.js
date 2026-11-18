// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The OpenGL ES 3.0 calls banner3ds-gles makes, as a WebGL 2 context. Rust calls these through its function table (src/gl.rs):
// every number arrives as a double (GL enums, ints, floats and addresses into the wasm memory all fit), and the names are the
// table's field names. GL objects are numbers on the Rust side: `objs` maps each to its WebGL object, per context.
let memory = null;
let ctx = null;
const states = new WeakMap();
const decoder = new TextDecoder();
const encoder = new TextEncoder();

export function init(mem) {
  memory = mem;
}

// Makes `gl` (a WebGL2RenderingContext, or null) the context every following call acts on.
export function set_context(gl) {
  if (!gl) {
    ctx = null;
    return;
  }
  let s = states.get(gl);
  if (!s) {
    s = { gl, objs: new Map(), locs: [], next: 1 };
    states.set(gl, s);
  }
  ctx = s;
}

const alloc = (object) => {
  const id = ctx.next++;
  ctx.objs.set(id, object);
  return id;
};
const obj = (id) => (id === 0 ? null : (ctx.objs.get(id) ?? null));
const bytes = () => new Uint8Array(memory.buffer);
const u32s = (ptr, n) => new Uint32Array(memory.buffer, ptr, n);
const i32s = (ptr, n) => new Int32Array(memory.buffer, ptr, n);
const f32s = (ptr, n) => new Float32Array(memory.buffer, ptr, n);
const cstr = (ptr) => {
  const b = bytes();
  let end = ptr;
  while (b[end] !== 0) end++;
  return decoder.decode(b.subarray(ptr, end));
};

export function viewport(x, y, w, h) { ctx.gl.viewport(x, y, w, h); }
export function scissor(x, y, w, h) { ctx.gl.scissor(x, y, w, h); }
export function clear_color(r, g, b, a) { ctx.gl.clearColor(r, g, b, a); }
export function clear(mask) { ctx.gl.clear(mask); }
export function color_mask(r, g, b, a) { ctx.gl.colorMask(!!r, !!g, !!b, !!a); }
export function enable(cap) { ctx.gl.enable(cap); }
export function disable(cap) { ctx.gl.disable(cap); }
export function depth_mask(flag) { ctx.gl.depthMask(!!flag); }
export function depth_func(func) { ctx.gl.depthFunc(func); }
export function cull_face(mode) { ctx.gl.cullFace(mode); }
export function blend_func_separate(sr, dr, sa, da) { ctx.gl.blendFuncSeparate(sr, dr, sa, da); }
export function blend_equation_separate(rgb, alpha) { ctx.gl.blendEquationSeparate(rgb, alpha); }

export function create_shader(kind) { return alloc(ctx.gl.createShader(kind)); }
export function shader_source(shader, count, strings, lengths) {
  const pointers = u32s(strings, count);
  const sizes = lengths ? i32s(lengths, count) : null;
  let source = '';
  for (let i = 0; i < count; i++) {
    const at = pointers[i];
    source += sizes && sizes[i] >= 0 ? decoder.decode(bytes().subarray(at, at + sizes[i])) : cstr(at);
  }
  ctx.gl.shaderSource(obj(shader), source);
}
export function compile_shader(shader) { ctx.gl.compileShader(obj(shader)); }
export function get_shaderiv(shader, pname, out) {
  const gl = ctx.gl;
  let v;
  if (pname === 0x8b84) v = logLength(gl.getShaderInfoLog(obj(shader)));
  else v = gl.getShaderParameter(obj(shader), pname);
  i32s(out, 1)[0] = typeof v === 'boolean' ? +v : v;
}
export function get_shader_info_log(shader, size, len, log) { writeLog(ctx.gl.getShaderInfoLog(obj(shader)), size, len, log); }
export function delete_shader(shader) { ctx.gl.deleteShader(obj(shader)); ctx.objs.delete(shader); }
export function create_program() { return alloc(ctx.gl.createProgram()); }
export function attach_shader(program, shader) { ctx.gl.attachShader(obj(program), obj(shader)); }
export function bind_attrib_location(program, index, name) { ctx.gl.bindAttribLocation(obj(program), index, cstr(name)); }
export function link_program(program) { ctx.gl.linkProgram(obj(program)); }
export function get_programiv(program, pname, out) {
  const gl = ctx.gl;
  let v;
  if (pname === 0x8b84) v = logLength(gl.getProgramInfoLog(obj(program)));
  else v = gl.getProgramParameter(obj(program), pname);
  i32s(out, 1)[0] = typeof v === 'boolean' ? +v : v;
}
export function get_program_info_log(program, size, len, log) { writeLog(ctx.gl.getProgramInfoLog(obj(program)), size, len, log); }
export function delete_program(program) { ctx.gl.deleteProgram(obj(program)); ctx.objs.delete(program); }
export function use_program(program) { ctx.gl.useProgram(obj(program)); }
export function get_uniform_location(program, name) {
  const loc = ctx.gl.getUniformLocation(obj(program), cstr(name));
  return loc ? ctx.locs.push(loc) - 1 : -1;
}
const loc = (i) => (i < 0 ? null : ctx.locs[i]);
export function uniform1i(l, v) { ctx.gl.uniform1i(loc(l), v); }
export function uniform1fv(l, count, p) { ctx.gl.uniform1fv(loc(l), f32s(p, count)); }
export function uniform2fv(l, count, p) { ctx.gl.uniform2fv(loc(l), f32s(p, count * 2)); }
export function uniform3fv(l, count, p) { ctx.gl.uniform3fv(loc(l), f32s(p, count * 3)); }
export function uniform4fv(l, count, p) { ctx.gl.uniform4fv(loc(l), f32s(p, count * 4)); }
export function uniform_matrix3fv(l, count, transpose, p) { ctx.gl.uniformMatrix3fv(loc(l), !!transpose, f32s(p, count * 9)); }
export function uniform_matrix4fv(l, count, transpose, p) { ctx.gl.uniformMatrix4fv(loc(l), !!transpose, f32s(p, count * 16)); }

function generate(n, out, make) {
  const ids = u32s(out, n);
  for (let i = 0; i < n; i++) ids[i] = alloc(make());
}
function remove(n, ptr, free) {
  const ids = Array.from(u32s(ptr, n));
  for (const id of ids) {
    free(obj(id));
    ctx.objs.delete(id);
  }
}
export function gen_vertex_arrays(n, out) { generate(n, out, () => ctx.gl.createVertexArray()); }
export function gen_buffers(n, out) { generate(n, out, () => ctx.gl.createBuffer()); }
export function gen_textures(n, out) { generate(n, out, () => ctx.gl.createTexture()); }
export function delete_vertex_arrays(n, ids) { remove(n, ids, (o) => ctx.gl.deleteVertexArray(o)); }
export function delete_buffers(n, ids) { remove(n, ids, (o) => ctx.gl.deleteBuffer(o)); }
export function delete_textures(n, ids) { remove(n, ids, (o) => ctx.gl.deleteTexture(o)); }
export function bind_vertex_array(id) { ctx.gl.bindVertexArray(obj(id)); }
export function bind_buffer(target, id) { ctx.gl.bindBuffer(target, obj(id)); }
export function buffer_data(target, size, data, usage) {
  if (data === 0) ctx.gl.bufferData(target, size, usage);
  else ctx.gl.bufferData(target, new Uint8Array(memory.buffer, data, size), usage);
}
export function buffer_sub_data(target, offset, size, data) {
  ctx.gl.bufferSubData(target, offset, new Uint8Array(memory.buffer, data, size));
}
export function enable_vertex_attrib_array(index) { ctx.gl.enableVertexAttribArray(index); }
export function vertex_attrib_pointer(index, size, kind, normalized, stride, offset) {
  ctx.gl.vertexAttribPointer(index, size, kind, !!normalized, stride, offset);
}
export function draw_elements(mode, count, kind, offset) { ctx.gl.drawElements(mode, count, kind, offset); }
export function active_texture(unit) { ctx.gl.activeTexture(unit); }
export function bind_texture(target, id) { ctx.gl.bindTexture(target, obj(id)); }
export function pixel_storei(pname, value) { ctx.gl.pixelStorei(pname, value); }
export function tex_image_2d(target, level, internal, w, h, border, format, kind, data) {
  let pixels = null;
  if (data !== 0) {
    if (format !== 0x1908) throw new Error('texImage2D: only RGBA uploads');
    if (kind === 0x1401) pixels = new Uint8Array(memory.buffer, data, w * h * 4);
    else if (kind === 0x1406) pixels = f32s(data, w * h * 4);
    else throw new Error('texImage2D: unsupported pixel type');
  }
  ctx.gl.texImage2D(target, level, internal, w, h, border, format, kind, pixels);
}
export function tex_parameteri(target, pname, value) { ctx.gl.texParameteri(target, pname, value); }

// GLES reports the log's length including the NUL; WebGL has no such query, so the length comes from the log itself.
const infoLogBytes = (text) => encoder.encode(text ?? '');
const logLength = (text) => {
  const n = infoLogBytes(text).length;
  return n ? n + 1 : 0;
};
function writeLog(text, size, lenPtr, logPtr) {
  const b = infoLogBytes(text);
  const n = Math.max(0, Math.min(b.length, size - 1));
  if (size > 0) {
    bytes().set(b.subarray(0, n), logPtr);
    bytes()[logPtr + n] = 0;
  }
  if (lenPtr) i32s(lenPtr, 1)[0] = n;
}
