import { init, set_context } from './snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js';
import { blob_size, read_range } from './snippets/gamemeta-wasm-2b79532adbb782ad/js/io.js';
import * as import1 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import2 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import3 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import4 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import5 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import6 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import7 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import8 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import9 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import10 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import11 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import12 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import13 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import14 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import15 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import16 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import17 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import18 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import19 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import20 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import21 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import22 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import23 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import24 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import25 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import26 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import27 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import28 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import29 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import30 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import31 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import32 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import33 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import34 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import35 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import36 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import37 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import38 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import39 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import40 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import41 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import42 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import43 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import44 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import45 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import46 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import47 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import48 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import49 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import50 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import51 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"
import * as import52 from "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js"


/**
 * The banner's pictures for a video: `render(index, fps, true)` is picture `index` over opaque black.
 */
export class ExportFrames {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ExportFramesFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_exportframes_free(ptr, 0);
    }
    /**
     * Whether anything moves (a still banner is one picture).
     * @returns {boolean}
     */
    animated() {
        const ret = wasm.exportframes_animated(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * How many pictures at `fps`.
     * @param {number} fps
     * @returns {number}
     */
    count(fps) {
        const ret = wasm.exportframes_count(this.__wbg_ptr, fps);
        return ret >>> 0;
    }
    /**
     * The picture height.
     * @returns {number}
     */
    height() {
        const ret = wasm.exportframes_height(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * The pictures at `scale` (1 to 3) times the on-screen size.
     * @param {Scene} scene
     * @param {number} scale
     */
    constructor(scene, scale) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(scene, Scene);
            wasm.exportframes_new(retptr, scene.__wbg_ptr, scale);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            ExportFramesFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Picture `index` at `fps`, RGBA, over opaque black when `opaque` and else over nothing.
     * @param {number} index
     * @param {number} fps
     * @param {boolean} opaque
     * @returns {Uint8Array}
     */
    render(index, fps, opaque) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.exportframes_render(retptr, this.__wbg_ptr, index, fps, opaque);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export4(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * The picture width.
     * @returns {number}
     */
    width() {
        const ret = wasm.exportframes_width(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) ExportFrames.prototype[Symbol.dispose] = ExportFrames.prototype.free;

/**
 * A GIF or animated WebP being made: `step()` until it returns false, then `finish()`.
 */
export class ExportJob {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ExportJobFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_exportjob_free(ptr, 0);
    }
    /**
     * How many are done.
     * @returns {number}
     */
    done() {
        const ret = wasm.exportjob_done(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * The finished file.
     * @returns {Uint8Array}
     */
    finish() {
        try {
            const ptr = this.__destroy_into_raw();
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.exportjob_finish(retptr, ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export4(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Starts a `"gif"` or `"webp"` export at `scale` (1 to 3) times the on-screen size.
     * @param {Scene} scene
     * @param {string} format
     * @param {number} scale
     */
    constructor(scene, format, scale) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(scene, Scene);
            const ptr0 = passStringToWasm0(format, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.exportjob_new(retptr, scene.__wbg_ptr, ptr0, len0, scale);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            ExportJobFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Draws and writes the next picture; false when there are no more.
     * @returns {boolean}
     */
    step() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.exportjob_step(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * How many pictures the export holds.
     * @returns {number}
     */
    total() {
        const ret = wasm.exportjob_total(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) ExportJob.prototype[Symbol.dispose] = ExportJob.prototype.free;

/**
 * The GLES renderer over a scene in one WebGL 2 context. Every method takes that context and makes it current first;
 * [`destroy`](Gles::destroy) must be called (with the context) before the object is dropped, or its GL objects leak.
 */
export class Gles {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        GlesFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_gles_free(ptr, 0);
    }
    /**
     * Deletes the GL objects. Once; later calls do nothing.
     * @param {any} gl
     */
    destroy(gl) {
        try {
            wasm.gles_destroy(this.__wbg_ptr, addBorrowedObject(gl));
        } finally {
            heap[stack_pointer++] = undefined;
        }
    }
    /**
     * Draws the banner at `frame` (60 to the second) into the whole `width` x `height` drawing buffer of `gl`, clearing it to
     * transparent black first.
     * @param {any} gl
     * @param {number} frame
     * @param {number} width
     * @param {number} height
     */
    draw(gl, frame, width, height) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.gles_draw(retptr, this.__wbg_ptr, addBorrowedObject(gl), frame, width, height);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            heap[stack_pointer++] = undefined;
        }
    }
    /**
     * Sets up programs, buffers and textures for `scene` in `gl` (a `WebGL2RenderingContext`); an error when nothing in the
     * scene can be drawn.
     * @param {Scene} scene
     * @param {any} gl
     */
    constructor(scene, gl) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(scene, Scene);
            wasm.gles_new(retptr, scene.__wbg_ptr, addBorrowedObject(gl));
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            GlesFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            heap[stack_pointer++] = undefined;
        }
    }
}
if (Symbol.dispose) Gles.prototype[Symbol.dispose] = Gles.prototype.free;

/**
 * What [`inspect`] found in one file or folder.
 */
export class Inspection {
    static __wrap(ptr) {
        const obj = Object.create(Inspection.prototype);
        obj.__wbg_ptr = ptr;
        InspectionFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        InspectionFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_inspection_free(ptr, 0);
    }
    /**
     * The large icon's RGBA bytes (its size is in `info.icon`), if any.
     * @returns {Uint8Array | undefined}
     */
    icon_rgba() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.inspection_icon_rgba(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            let v1;
            if (r0 !== 0) {
                v1 = getArrayU8FromWasm0(r0, r1).slice();
                wasm.__wbindgen_export4(r0, r1 * 1, 1);
            }
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `{name, size, known, format, info, ids, ra, banner, zip?}` as JSON (`known`: a reader exists for this file name; `zip`: the ROM read and the zip's others): `info` and `ids` are the CLI's objects (`null` when the file
     * gives none), `ra` says whether a RetroAchievements hash applies, `banner` is `{state, ...}` (see `banner_json`).
     * @returns {string}
     */
    json() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.inspection_json(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export4(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * The small icon's RGBA bytes (its size is in `info.smallIcon`), if any.
     * @returns {Uint8Array | undefined}
     */
    small_icon_rgba() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.inspection_small_icon_rgba(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            let v1;
            if (r0 !== 0) {
                v1 = getArrayU8FromWasm0(r0, r1).slice();
                wasm.__wbindgen_export4(r0, r1 * 1, 1);
            }
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * The banner scene, once (`None` when the file has none or it was already taken).
     * @returns {Scene | undefined}
     */
    take_scene() {
        const ret = wasm.inspection_take_scene(this.__wbg_ptr);
        return ret === 0 ? undefined : Scene.__wrap(ret);
    }
}
if (Symbol.dispose) Inspection.prototype[Symbol.dispose] = Inspection.prototype.free;

/**
 * An MP4 being muxed from the page's encoded video (and audio) chunks.
 */
export class Mp4 {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        Mp4Finalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_mp4_free(ptr, 0);
    }
    /**
     * The finished file.
     * @returns {Uint8Array}
     */
    finish() {
        try {
            const ptr = this.__destroy_into_raw();
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.mp4_finish(retptr, ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export4(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `avcc` is the video encoder's `description`; sample durations are in 1/`timescale` s.
     * @param {number} width
     * @param {number} height
     * @param {number} timescale
     * @param {Uint8Array} avcc
     */
    constructor(width, height, timescale, avcc) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArray8ToWasm0(avcc, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            wasm.mp4_new(retptr, width, height, timescale, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            Mp4Finalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Adds an audio sample of `duration` samples of the audio rate.
     * @param {Uint8Array} data
     * @param {number} duration
     */
    push_audio(data, duration) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            wasm.mp4_push_audio(retptr, this.__wbg_ptr, ptr0, len0, duration);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Adds a video sample.
     * @param {Uint8Array} data
     * @param {number} duration
     * @param {boolean} key
     */
    push_video(data, duration, key) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            wasm.mp4_push_video(retptr, this.__wbg_ptr, ptr0, len0, duration, key);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Adds an AAC track; `config` is the audio encoder's `description`.
     * @param {Uint8Array} config
     * @param {number} rate
     * @param {number} channels
     */
    set_aac(config, rate, channels) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArray8ToWasm0(config, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            wasm.mp4_set_aac(retptr, this.__wbg_ptr, ptr0, len0, rate, channels);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Adds an Opus track (48 kHz).
     * @param {number} pre_skip
     * @param {number} input_rate
     * @param {number} channels
     */
    set_opus(pre_skip, input_rate, channels) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.mp4_set_opus(retptr, this.__wbg_ptr, pre_skip, input_rate, channels);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) Mp4.prototype[Symbol.dispose] = Mp4.prototype.free;

/**
 * The RetroAchievements hash of one file, fed in order: `RaHash.new(name, size)`, `update` with each chunk, `finish`.
 */
export class RaHash {
    static __wrap(ptr) {
        const obj = Object.create(RaHash.prototype);
        obj.__wbg_ptr = ptr;
        RaHashFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        RaHashFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_rahash_free(ptr, 0);
    }
    /**
     * The lower-case hex MD5, or `None` when the file is not hashable this way. Once.
     * @returns {string | undefined}
     */
    finish() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.rahash_finish(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            let v1;
            if (r0 !== 0) {
                v1 = getStringFromWasm0(r0, r1);
                wasm.__wbindgen_export4(r0, r1 * 1, 1);
            }
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * A hasher for `name` of `size` bytes, or `None` when the file's platform is not hashed.
     * @param {string} name
     * @param {number} size
     * @returns {RaHash | undefined}
     */
    static start(name, size) {
        const ptr0 = passStringToWasm0(name, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.rahash_start(ptr0, len0, size);
        return ret === 0 ? undefined : RaHash.__wrap(ret);
    }
    /**
     * The next bytes of the file.
     * @param {Uint8Array} chunk
     */
    update(chunk) {
        const ptr0 = passArray8ToWasm0(chunk, wasm.__wbindgen_export);
        const len0 = WASM_VECTOR_LEN;
        wasm.rahash_update(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) RaHash.prototype[Symbol.dispose] = RaHash.prototype.free;

/**
 * A banner scene, ready to draw.
 */
export class Scene {
    static __wrap(ptr) {
        const obj = Object.create(Scene.prototype);
        obj.__wbg_ptr = ptr;
        SceneFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        SceneFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_scene_free(ptr, 0);
    }
    /**
     * Whether anything moves.
     * @returns {boolean}
     */
    animated() {
        const ret = wasm.scene_animated(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * The loop length in frames (60 to the second); 0 when the banner does not animate.
     * @returns {number}
     */
    frames() {
        const ret = wasm.scene_frames(this.__wbg_ptr);
        return ret;
    }
    /**
     * Whether the banner has a jingle that decodes.
     * @returns {boolean}
     */
    has_sound() {
        const ret = wasm.scene_has_sound(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * The jingle's channel count (0 when there is none).
     * @returns {number}
     */
    sound_channels() {
        const ret = wasm.scene_sound_channels(this.__wbg_ptr);
        return ret;
    }
    /**
     * The jingle as interleaved 16-bit samples at `rate` Hz (the file's own rate when `rate` is 0), cut to `seconds`
     * (no cut when negative); empty when there is none.
     * @param {number} rate
     * @param {number} seconds
     * @returns {Int16Array}
     */
    sound_pcm(rate, seconds) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.scene_sound_pcm(retptr, this.__wbg_ptr, rate, seconds);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayI16FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export4(r0, r1 * 2, 2);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * The jingle's sample rate (0 when there is none).
     * @returns {number}
     */
    sound_rate() {
        const ret = wasm.scene_sound_rate(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) Scene.prototype[Symbol.dispose] = Scene.prototype.free;

/**
 * The CPU renderer over a scene: `render(frame, width, height)` gives `width * height` RGBA bytes, top row first.
 */
export class SoftRenderer {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        SoftRendererFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_softrenderer_free(ptr, 0);
    }
    /**
     * @param {Scene} scene
     */
    constructor(scene) {
        _assertClass(scene, Scene);
        const ret = wasm.softrenderer_new(scene.__wbg_ptr);
        this.__wbg_ptr = ret;
        SoftRendererFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * The banner at `frame` (60 to the second), `width` x `height`, through HOME's view over transparent black.
     * @param {number} frame
     * @param {number} width
     * @param {number} height
     * @returns {Uint8Array}
     */
    render(frame, width, height) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.softrenderer_render(retptr, this.__wbg_ptr, frame, width, height);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export4(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) SoftRenderer.prototype[Symbol.dispose] = SoftRenderer.prototype.free;

/**
 * Reads the file `blob` (called `name`) as the library would on any host: the metadata, the ids (not the RetroAchievements
 * hash, which reads the whole file: [`RaHash`]) and the 3DS banner scene. `open` (optional) maps the path of a file beside
 * it (a cue sheet's track) to a `Blob` or `undefined`. For a zip, `entry` names the ROM inside it to read (the first ROM when
 * absent); the result's `zip` object lists the zip's other ROMs so the page can ask for each.
 * @param {any} blob
 * @param {string} name
 * @param {Function | null} [open]
 * @param {string | null} [entry]
 * @returns {Inspection}
 */
export function inspect(blob, name, open, entry) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_export, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    var ptr1 = isLikeNone(entry) ? 0 : passStringToWasm0(entry, wasm.__wbindgen_export, wasm.__wbindgen_export2);
    var len1 = WASM_VECTOR_LEN;
    const ret = wasm.inspect(addHeapObject(blob), ptr0, len0, isLikeNone(open) ? 0 : addHeapObject(open), ptr1, len1);
    return Inspection.__wrap(ret);
}

/**
 * Reads a picked folder as a Wii U title: `paths` are the files' paths relative to the folder's parent (`webkitRelativePath`),
 * `blobs` the matching `File`s. `format` is `WiiUTitle` when a `meta/meta.xml` is among them, else everything is empty.
 * @param {string[]} paths
 * @param {Array<any>} blobs
 * @param {string} name
 * @returns {Inspection}
 */
export function inspect_folder(paths, blobs, name) {
    const ptr0 = passArrayJsValueToWasm0(paths, wasm.__wbindgen_export);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(name, wasm.__wbindgen_export, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.inspect_folder(ptr0, len0, addHeapObject(blobs), ptr1, len1);
    return Inspection.__wrap(ret);
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_30c8987f7c2ed4e2: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return addHeapObject(ret);
        },
        __wbg___wbindgen_is_null_e343b7d08827ba72: function(arg0) {
            const ret = getObject(arg0) === null;
            return ret;
        },
        __wbg___wbindgen_is_undefined_8865fb403f8fe9d8: function(arg0) {
            const ret = getObject(arg0) === undefined;
            return ret;
        },
        __wbg___wbindgen_memory_caa4a6165639c8b5: function() {
            const ret = wasm.memory;
            return addHeapObject(ret);
        },
        __wbg___wbindgen_string_get_0380ccaa2f57f0d9: function(arg0, arg1) {
            const obj = getObject(arg1);
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_41e9ee4f547fc59a: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_blob_size_4d0a513f2f3efef5: function(arg0) {
            const ret = blob_size(getObject(arg0));
            return ret;
        },
        __wbg_call_187d372bd5fdd4aa: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = getObject(arg0).call(getObject(arg1), getObject(arg2));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_get_unchecked_288889d017702237: function(arg0, arg1) {
            const ret = getObject(arg0)[arg1 >>> 0];
            return addHeapObject(ret);
        },
        __wbg_init_f65b82d45ee39070: function(arg0) {
            init(takeObject(arg0));
        },
        __wbg_length_7f3c00c40364105e: function(arg0) {
            const ret = getObject(arg0).length;
            return ret;
        },
        __wbg_length_d4bdea10311bd9cf: function(arg0) {
            const ret = getObject(arg0).length;
            return ret;
        },
        __wbg_prototypesetcall_bc27214492979395: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), getObject(arg2));
        },
        __wbg_read_range_37e84828adce36a9: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = read_range(getObject(arg0), arg1, arg2 >>> 0);
            return addHeapObject(ret);
        }, arguments); },
        __wbg_set_context_b7f85ef8fe1826b7: function(arg0) {
            set_context(getObject(arg0));
        },
        __wbindgen_generic_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return addHeapObject(ret);
        },
        __wbindgen_object_clone_ref: function(arg0) {
            const ret = getObject(arg0);
            return addHeapObject(ret);
        },
        __wbindgen_object_drop_ref: function(arg0) {
            takeObject(arg0);
        },
    };
    return {
        __proto__: null,
        "./gamemeta_wasm_bg.js": import0,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import1,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import2,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import3,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import4,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import5,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import6,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import7,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import8,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import9,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import10,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import11,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import12,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import13,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import14,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import15,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import16,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import17,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import18,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import19,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import20,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import21,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import22,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import23,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import24,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import25,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import26,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import27,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import28,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import29,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import30,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import31,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import32,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import33,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import34,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import35,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import36,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import37,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import38,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import39,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import40,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import41,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import42,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import43,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import44,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import45,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import46,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import47,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import48,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import49,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import50,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import51,
        "./snippets/gamemeta-wasm-2b79532adbb782ad/js/gl.js": import52,
    };
}

const ExportFramesFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_exportframes_free(ptr, 1));
const ExportJobFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_exportjob_free(ptr, 1));
const GlesFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_gles_free(ptr, 1));
const InspectionFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_inspection_free(ptr, 1));
const Mp4Finalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_mp4_free(ptr, 1));
const RaHashFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_rahash_free(ptr, 1));
const SceneFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_scene_free(ptr, 1));
const SoftRendererFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_softrenderer_free(ptr, 1));

function addHeapObject(obj) {
    if (heap_next === heap.length) heap.push(heap.length + 1);
    const idx = heap_next;
    heap_next = heap[idx];

    heap[idx] = obj;
    return idx;
}

function _assertClass(instance, klass) {
    if (!(instance instanceof klass)) {
        throw new Error(`expected instance of ${klass.name}`);
    }
}

function addBorrowedObject(obj) {
    if (stack_pointer == 1) throw new Error('out of js stack');
    heap[--stack_pointer] = obj;
    return stack_pointer;
}

function dropObject(idx) {
    if (idx < 1028) return;
    heap[idx] = heap_next;
    heap_next = idx;
}

function getArrayI16FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getInt16ArrayMemory0().subarray(ptr / 2, ptr / 2 + len);
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

let cachedInt16ArrayMemory0 = null;
function getInt16ArrayMemory0() {
    if (cachedInt16ArrayMemory0 === null || cachedInt16ArrayMemory0.byteLength === 0) {
        cachedInt16ArrayMemory0 = new Int16Array(wasm.memory.buffer);
    }
    return cachedInt16ArrayMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function getObject(idx) { return heap[idx]; }

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        wasm.__wbindgen_export3(addHeapObject(e));
    }
}

let heap = new Array(1024).fill(undefined);
heap.push(undefined, null, true, false);

let heap_next = heap.length;

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    const mem = getDataViewMemory0();
    for (let i = 0; i < array.length; i++) {
        mem.setUint32(ptr + 4 * i, addHeapObject(array[i]), true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

let stack_pointer = 1024;

function takeObject(idx) {
    const ret = getObject(idx);
    dropObject(idx);
    return ret;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedInt16ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('gamemeta_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
