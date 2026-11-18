// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! JNI entry points for `gamemeta.GameMeta` (bindings/kotlin). The Kotlin side owns the API; these functions move
//! plain values across: a handle (a boxed `GameInfo`, or `-GmStatus`), strings, pixels and numbers.
//! Every entry point catches panics. The library also carries the C ABI of `gamemeta-ffi`, so native code in the
//! same process can call `gm_*` directly.

use std::panic::{AssertUnwindSafe, catch_unwind};

use gamemeta::ids::{GmIds, read_ids};
use gamemeta::scene::{GmScene, read_scene};
use gamemeta::wii_banner::{GmWiiBanner, GmWiiBannerCount, read_banner};
use gamemeta::{GM_ABI_VERSION, GmStatus, image_kind, read_tracked, wua_image};
use gamemeta_core::ids::{DiscIdRefusal, DiscIdResult, DiscRegion, DiscSystem, Opener, RaHasher};
use gamemeta_core::wiiu::{self, TitleFiles};
use gamemeta_core::{ByteSource, GameInfo, Language, ReadError, RgbaImage, SliceSource};
use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JObject, JString, JValue};
use jni::sys::{jint, jlong, jlongArray, jobjectArray};

/// Strings per handle: 12 languages x (name, short name, publisher), then title id, product code, game id, maker
/// code, header title.
const STRINGS: usize = Language::ALL.len() * 3 + 6;

fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// Statuses cross as `-status`, in `-MAX_STATUS..=0`; every other value is a pointer. A sign test is not enough:
/// Android's arm64 allocator tags heap pointers in the top byte (0xb4..), which makes them negative as a `jlong`.
const MAX_STATUS: jlong = 255;

/// Whether `handle` is a pointer from [`into_handle`] rather than a status.
fn is_pointer(handle: jlong) -> bool {
    !(-MAX_STATUS..=0).contains(&handle)
}

fn into_handle(result: Result<GameInfo, GmStatus>) -> jlong {
    match result {
        Ok(info) => Box::into_raw(Box::new(info)) as jlong,
        Err(status) => -(status as jlong),
    }
}

/// The handle's `GameInfo`; `None` for a status.
///
/// Safety: a pointer `handle` came from `into_handle` and has not been freed.
unsafe fn info<'a>(handle: jlong) -> Option<&'a GameInfo> {
    // SAFETY: caller contract.
    is_pointer(handle).then(|| unsafe { &*(handle as *const GameInfo) })
}

fn name(env: &mut JNIEnv, s: &JString) -> Option<String> {
    env.get_string(s).ok().map(String::from)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeAbiVersion(_env: JNIEnv, _class: JClass) -> jint {
    GM_ABI_VERSION as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeReadBytes(
    mut env: JNIEnv,
    _class: JClass,
    bytes: JByteArray,
    file_name: JString,
) -> jlong {
    guard(-(GmStatus::ErrPanic as jlong), || {
        let (Some(file_name), Ok(bytes)) = (name(&mut env, &file_name), env.convert_byte_array(&bytes)) else {
            return -(GmStatus::ErrArgument as jlong);
        };
        into_handle(read_tracked(SliceSource::new(&bytes), &file_name))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeReadFd(
    mut env: JNIEnv,
    _class: JClass,
    fd: jint,
    file_name: JString,
) -> jlong {
    guard(-(GmStatus::ErrPanic as jlong), || {
        let Some(file_name) = name(&mut env, &file_name) else { return -(GmStatus::ErrArgument as jlong) };
        into_handle(gamemeta::fd::read_fd(fd, &file_name))
    })
}

/// A Wii U title's files served by a Kotlin `gamemeta.TitleFiles`: `size(path): Long` (-1 when there is no such file)
/// and `read(path, maxBytes): ByteArray?` (null when missing or over the cap). An exception thrown by either is an
/// I/O failure: it is cleared here and reported as [`GmStatus::ErrIo`].
struct JniFiles<'a, 'e, 'o> {
    env: &'a mut JNIEnv<'e>,
    files: &'a JObject<'o>,
    failed: bool,
}

impl JniFiles<'_, '_, '_> {
    /// The call's result, or `None` after recording a failed call (a pending exception is cleared).
    fn settle<T>(&mut self, result: jni::errors::Result<T>) -> Option<T> {
        match result {
            Ok(v) => Some(v),
            Err(_) => {
                if self.env.exception_check().unwrap_or(false) {
                    let _ = self.env.exception_clear();
                }
                self.failed = true;
                None
            }
        }
    }
}

impl TitleFiles for JniFiles<'_, '_, '_> {
    fn size(&mut self, path: &str) -> Option<u64> {
        let jpath = self.env.new_string(path).ok()?;
        let call = self.env.call_method(self.files, "size", "(Ljava/lang/String;)J", &[JValue::Object(&jpath)]);
        let _ = self.env.delete_local_ref(jpath);
        let size = self.settle(call.and_then(|v| v.j()))?;
        u64::try_from(size).ok()
    }

    fn read(&mut self, path: &str, max: usize) -> Option<Vec<u8>> {
        let jpath = self.env.new_string(path).ok()?;
        let cap = jni::sys::jint::try_from(max).unwrap_or(jni::sys::jint::MAX);
        let call = self.env.call_method(
            self.files,
            "read",
            "(Ljava/lang/String;I)[B",
            &[JValue::Object(&jpath), JValue::Int(cap)],
        );
        let _ = self.env.delete_local_ref(jpath);
        let object = self.settle(call.and_then(|v| v.l()))?;
        if object.is_null() {
            return None;
        }
        let array = JByteArray::from(object);
        let bytes = self.env.convert_byte_array(&array).ok();
        let _ = self.env.delete_local_ref(array);
        bytes.filter(|b| b.len() <= max)
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeReadTitle(
    mut env: JNIEnv,
    _class: JClass,
    files: JObject,
) -> jlong {
    guard(-(GmStatus::ErrPanic as jlong), || {
        let mut files = JniFiles { env: &mut env, files: &files, failed: false };
        let info = wiiu::read_title(&mut files);
        into_handle(match info {
            Some(info) => Ok(info),
            None if files.failed => Err(GmStatus::ErrIo),
            None => Err(GmStatus::NotFound),
        })
    })
}

/// A picture as bytes: width and height as two big-endian `u32`, then the RGBA8 pixels.
fn encode_image(image: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + image.pixels.len());
    out.extend(image.width.to_be_bytes());
    out.extend(image.height.to_be_bytes());
    out.extend(&image.pixels);
    out
}

/// Hands a decoded picture to Kotlin: the bytes of [`encode_image`], null when there is none (`NotFound`, a bad
/// `kind`), or a thrown `IOException` when the source failed.
fn return_image(env: &mut JNIEnv, result: Result<RgbaImage, GmStatus>) -> jni::sys::jbyteArray {
    match result {
        Ok(image) => env.byte_array_from_slice(&encode_image(&image)).map_or(std::ptr::null_mut(), |a| a.into_raw()),
        Err(GmStatus::ErrIo) => {
            let _ = env.throw_new("java/io/IOException", "game-metadata: the file could not be read");
            std::ptr::null_mut()
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// One picture of a title served by `files` (a `gamemeta.TitleFiles`); `kind` is an `ImageKind` bit.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeTitleImage(
    mut env: JNIEnv,
    _class: JClass,
    files: JObject,
    kind: jint,
) -> jni::sys::jbyteArray {
    guard(std::ptr::null_mut(), || {
        let Some(kind) = u32::try_from(kind).ok().and_then(image_kind) else { return std::ptr::null_mut() };
        let (result, failed) = {
            let mut files = JniFiles { env: &mut env, files: &files, failed: false };
            let image = wiiu::image(&mut files, kind);
            (image, files.failed)
        };
        return_image(&mut env, result.ok_or(if failed { GmStatus::ErrIo } else { GmStatus::NotFound }))
    })
}

/// One picture of the base title of a `.wua` held in `bytes`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeWuaImageBytes(
    mut env: JNIEnv,
    _class: JClass,
    bytes: JByteArray,
    kind: jint,
) -> jni::sys::jbyteArray {
    guard(std::ptr::null_mut(), || {
        let (Some(kind), Ok(bytes)) = (u32::try_from(kind).ok().and_then(image_kind), env.convert_byte_array(&bytes))
        else {
            return std::ptr::null_mut();
        };
        return_image(&mut env, wua_image(SliceSource::new(&bytes), kind))
    })
}

/// One picture of the base title of the `.wua` in the open descriptor `fd` (borrowed).
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeWuaImageFd(
    mut env: JNIEnv,
    _class: JClass,
    fd: jint,
    kind: jint,
) -> jni::sys::jbyteArray {
    guard(std::ptr::null_mut(), || {
        let Some(kind) = u32::try_from(kind).ok().and_then(image_kind) else { return std::ptr::null_mut() };
        return_image(&mut env, gamemeta::fd::wua_image_fd(fd, kind))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeStrings(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jobjectArray {
    guard(std::ptr::null_mut(), || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(info) = (unsafe { info(handle) }) else { return std::ptr::null_mut() };
        let mut values: Vec<Option<&str>> = Vec::with_capacity(STRINGS);
        for language in Language::ALL {
            let t = info.title_in(language);
            values.push(t.map(|t| t.name.as_str()));
            values.push(t.map(|t| t.short_name.as_str()));
            values.push(t.and_then(|t| t.publisher.as_deref()));
        }
        values.push(info.ids.title_id.as_deref());
        values.push(info.ids.product_code.as_deref());
        values.push(info.ids.game_id.as_deref());
        values.push(info.ids.maker_code.as_deref());
        values.push(info.header_title.as_deref());
        let cart_json = info.cart.as_ref().map(|c| c.to_json());
        values.push(cart_json.as_deref());
        let Ok(array) = env.new_object_array(STRINGS as i32, "java/lang/String", JObject::null()) else {
            return std::ptr::null_mut();
        };
        for (i, v) in values.into_iter().enumerate() {
            if let Some(s) = v {
                let Ok(js) = env.new_string(s) else { return std::ptr::null_mut() };
                if env.set_object_array_element(&array, i as i32, &js).is_err() {
                    return std::ptr::null_mut();
                }
                let _ = env.delete_local_ref(js);
            }
        }
        array.into_raw()
    })
}

/// The game's version (a cartridge header's version byte or revision), -1 when the file has none.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeVersion(_env: JNIEnv, _class: JClass, handle: jlong) -> jlong {
    guard(-1, || {
        // SAFETY: the Kotlin side passes live handles only.
        unsafe { info(handle) }.and_then(|i| i.version).map_or(-1, i64::from)
    })
}

/// Numbers per handle, in this order: platform, region mask, region free (0/1), preferred language (-1 none),
/// banner offset and size (-1 none), large icon width and height, small icon width and height (0 none), then the
/// age for each of the 16 rating slots (-1 none), the image mask (`1 << ImageKind`), the Wii U boot movie and sound
/// sizes (-1 none), the animated icon's step count, then per step its width, height and duration in 1/60 s.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeNumbers(env: JNIEnv, _class: JClass, handle: jlong) -> jlongArray {
    guard(std::ptr::null_mut(), || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(info) = (unsafe { info(handle) }) else { return std::ptr::null_mut() };
        let size =
            |im: &Option<gamemeta_core::RgbaImage>| im.as_ref().map_or([0, 0], |i| [i.width as i64, i.height as i64]);
        let mut v: Vec<i64> = vec![
            info.platform as i64,
            info.regions.iter().fold(0, |m, r| m | 1 << *r as u32),
            info.region_free as i64,
            info.preferred_title().map_or(-1, |(l, _)| l as i64),
            info.banner.map_or(-1, |b| b.offset as i64),
            info.banner.map_or(-1, |b| b.size as i64),
        ];
        v.extend(size(&info.icon));
        v.extend(size(&info.small_icon));
        let mut ratings = [-1i64; 16];
        for (board, age) in &info.ratings {
            ratings[*board as usize] = i64::from(*age);
        }
        v.extend(ratings);
        v.push(info.images.iter().fold(0, |m, k| m | 1 << *k as u32));
        v.push(info.boot_movie.map_or(-1, |s| s as i64));
        v.push(info.boot_sound.map_or(-1, |s| s as i64));
        v.push(info.animated_icon.len() as i64);
        for step in &info.animated_icon {
            v.extend([step.frame.width as i64, step.frame.height as i64, i64::from(step.duration_frames)]);
        }
        let Ok(array) = env.new_long_array(v.len() as i32) else { return std::ptr::null_mut() };
        if env.set_long_array_region(&array, 0, &v).is_err() {
            return std::ptr::null_mut();
        }
        array.into_raw()
    })
}

/// An icon's RGBA8 pixels: `which` 0 = large, 1 = small, 2 + n = step n of the animated icon; null when absent.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeIcon(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    which: jint,
) -> jni::sys::jbyteArray {
    guard(std::ptr::null_mut(), || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(info) = (unsafe { info(handle) }) else { return std::ptr::null_mut() };
        let image = match which {
            0 => info.icon.as_ref(),
            1 => info.small_icon.as_ref(),
            n => info.animated_icon.get(n as usize - 2).map(|s| &s.frame),
        };
        image.and_then(|im| env.byte_array_from_slice(&im.pixels).ok()).map_or(std::ptr::null_mut(), |a| a.into_raw())
    })
}

/// A scene handle, as [`into_handle`] makes one: a boxed `GmScene`, or `-GmStatus`.
fn into_scene_handle(result: Result<GmScene, GmStatus>) -> jlong {
    match result {
        Ok(scene) => Box::into_raw(Box::new(scene)) as jlong,
        Err(status) => -(status as jlong),
    }
}

/// The handle's `GmScene`; `None` for a status.
///
/// Safety: a pointer `handle` came from `into_scene_handle` and has not been freed.
unsafe fn scene<'a>(handle: jlong) -> Option<&'a GmScene> {
    // SAFETY: caller contract.
    is_pointer(handle).then(|| unsafe { &*(handle as *const GmScene) })
}

/// The banner scene of the 3DS banner or file in `bytes` (a handle, or `-status`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeSceneBytes(
    env: JNIEnv,
    _class: JClass,
    bytes: JByteArray,
) -> jlong {
    guard(-(GmStatus::ErrPanic as jlong), || match env.convert_byte_array(&bytes) {
        Ok(bytes) => into_scene_handle(read_scene(SliceSource::new(&bytes))),
        Err(_) => -(GmStatus::ErrArgument as jlong),
    })
}

/// The banner scene of the open descriptor `fd` (borrowed).
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeSceneFd(_env: JNIEnv, _class: JClass, fd: jint) -> jlong {
    guard(-(GmStatus::ErrPanic as jlong), || into_scene_handle(gamemeta::fd::scene_fd(fd)))
}

/// Strings of a scene handle: its JSON, then the texture names, then the unsupported-feature flags (the counts are in
/// [`Java_gamemeta_GameMeta_nativeSceneNumbers`]).
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeSceneStrings(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jobjectArray {
    guard(std::ptr::null_mut(), || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(scene) = (unsafe { scene(handle) }) else { return std::ptr::null_mut() };
        let Some(json) = scene.json().and_then(|j| j.to_str().ok()) else { return std::ptr::null_mut() };
        let mut values: Vec<&str> = vec![json];
        values.extend(scene.scene().textures.keys().map(String::as_str));
        values.extend(scene.scene().unsupported.iter().map(String::as_str));
        let Ok(array) = env.new_object_array(values.len() as i32, "java/lang/String", JObject::null()) else {
            return std::ptr::null_mut();
        };
        for (i, v) in values.into_iter().enumerate() {
            let Ok(js) = env.new_string(v) else { return std::ptr::null_mut() };
            if env.set_object_array_element(&array, i as i32, &js).is_err() {
                return std::ptr::null_mut();
            }
            let _ = env.delete_local_ref(js);
        }
        array.into_raw()
    })
}

/// Counts of a scene handle, in `GmSceneCount` order: models, meshes, textures, lights, cameras, skeletal, material
/// and visibility animations, unsupported flags.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeSceneNumbers(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jlongArray {
    guard(std::ptr::null_mut(), || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(scene) = (unsafe { scene(handle) }) else { return std::ptr::null_mut() };
        let v: Vec<i64> = (0..9).map(|what| i64::from(scene.count(what))).collect();
        let Ok(array) = env.new_long_array(v.len() as i32) else { return std::ptr::null_mut() };
        if env.set_long_array_region(&array, 0, &v).is_err() {
            return std::ptr::null_mut();
        }
        array.into_raw()
    })
}

/// The `index`th texture (sorted by name) as [`encode_image`]; null when out of range.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeSceneTexture(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    index: jint,
) -> jni::sys::jbyteArray {
    guard(std::ptr::null_mut(), || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(scene) = (unsafe { scene(handle) }) else { return std::ptr::null_mut() };
        let image = usize::try_from(index).ok().and_then(|i| scene.scene().textures.values().nth(i));
        image
            .and_then(|im| env.byte_array_from_slice(&encode_image(im)).ok())
            .map_or(std::ptr::null_mut(), |a| a.into_raw())
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeSceneFree(_env: JNIEnv, _class: JClass, handle: jlong) {
    if is_pointer(handle) {
        // SAFETY: the Kotlin side frees each handle once, in a finally block.
        guard((), || drop(unsafe { Box::from_raw(handle as *mut GmScene) }));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeFree(_env: JNIEnv, _class: JClass, handle: jlong) {
    free(handle);
}

fn free(handle: jlong) {
    if is_pointer(handle) {
        // SAFETY: the Kotlin side frees each handle once, in a finally block.
        guard((), || drop(unsafe { Box::from_raw(handle as *mut GameInfo) }));
    }
}

// ---- ids

fn ids_handle(result: Result<GmIds, GmStatus>) -> jlong {
    match result {
        Ok(ids) => Box::into_raw(Box::new(ids)) as jlong,
        Err(status) => -(status as jlong),
    }
}

/// A file opened by a Kotlin `gamemeta.FileSource` (`getSize(): Long`, `read(offset: Long, length: Int): ByteArray?` that
/// is null for a range outside the file, `close()`). An exception thrown by `read` is an I/O failure of the source.
struct JniSource<'a, 'e> {
    env: &'a mut JNIEnv<'e>,
    file: JObject<'e>,
    size: u64,
}

impl ByteSource for JniSource<'_, '_> {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        let (Ok(offset), Ok(len)) = (jlong::try_from(offset), jint::try_from(buf.len())) else {
            return Err(ReadError::OutOfRange);
        };
        let call = self.env.call_method(&self.file, "read", "(JI)[B", &[JValue::Long(offset), JValue::Int(len)]);
        let object = match call.and_then(|v| v.l()) {
            Ok(o) => o,
            Err(_) => {
                if self.env.exception_check().unwrap_or(false) {
                    let _ = self.env.exception_clear();
                }
                return Err(ReadError::Io);
            }
        };
        if object.is_null() {
            return Err(ReadError::OutOfRange);
        }
        let array = JByteArray::from(object);
        let bytes = self.env.convert_byte_array(&array);
        let _ = self.env.delete_local_ref(array);
        match bytes {
            Ok(b) if b.len() == buf.len() => {
                buf.copy_from_slice(&b);
                Ok(())
            }
            Ok(_) => Err(ReadError::OutOfRange),
            Err(_) => Err(ReadError::Io),
        }
    }
}

impl Drop for JniSource<'_, '_> {
    fn drop(&mut self) {
        let _ = self.env.call_method(&self.file, "close", "()V", &[]);
        if self.env.exception_check().unwrap_or(false) {
            let _ = self.env.exception_clear();
        }
        let file = std::mem::replace(&mut self.file, JObject::null());
        let _ = self.env.delete_local_ref(file);
    }
}

/// Files beside the game file served by a Kotlin `gamemeta.FileOpener` (`open(path: String): FileSource?`), or none.
struct JniOpener<'a, 'e, 'o> {
    env: &'a mut JNIEnv<'e>,
    opener: Option<&'a JObject<'o>>,
}

impl Opener for JniOpener<'_, '_, '_> {
    fn open(&mut self, path: &str) -> Option<Box<dyn ByteSource + '_>> {
        let opener = self.opener?;
        let jpath = self.env.new_string(path).ok()?;
        let call = self.env.call_method(
            opener,
            "open",
            "(Ljava/lang/String;)Lgamemeta/FileSource;",
            &[JValue::Object(&jpath)],
        );
        let _ = self.env.delete_local_ref(jpath);
        let file = match call.and_then(|v| v.l()) {
            Ok(f) => f,
            Err(_) => {
                if self.env.exception_check().unwrap_or(false) {
                    let _ = self.env.exception_clear();
                }
                return None;
            }
        };
        if file.is_null() {
            return None;
        }
        let size = self.env.call_method(&file, "getSize", "()J", &[]).and_then(|v| v.j());
        let Some(size) = size.ok().and_then(|s| u64::try_from(s).ok()) else {
            if self.env.exception_check().unwrap_or(false) {
                let _ = self.env.exception_clear();
            }
            let _ = self.env.delete_local_ref(file);
            return None;
        };
        Some(Box::new(JniSource { env: &mut *self.env, file, size }))
    }
}

/// The ids of `bytes`, a file called `file_name`; `opener` is a `gamemeta.FileOpener` or null (a handle, or `-status`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeIdsBytes(
    mut env: JNIEnv,
    _class: JClass,
    bytes: JByteArray,
    file_name: JString,
    opener: JObject,
) -> jlong {
    guard(-(GmStatus::ErrPanic as jlong), || {
        let (Some(file_name), Ok(bytes)) = (name(&mut env, &file_name), env.convert_byte_array(&bytes)) else {
            return -(GmStatus::ErrArgument as jlong);
        };
        let mut files = JniOpener { env: &mut env, opener: (!opener.is_null()).then_some(&opener) };
        ids_handle(read_ids(SliceSource::new(&bytes), &file_name, &mut files))
    })
}

/// [`Java_gamemeta_GameMeta_nativeIdsBytes`] for the open descriptor `fd` (borrowed).
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeIdsFd(
    mut env: JNIEnv,
    _class: JClass,
    fd: jint,
    file_name: JString,
    opener: JObject,
) -> jlong {
    guard(-(GmStatus::ErrPanic as jlong), || {
        let Some(file_name) = name(&mut env, &file_name) else { return -(GmStatus::ErrArgument as jlong) };
        let mut files = JniOpener { env: &mut env, opener: (!opener.is_null()).then_some(&opener) };
        ids_handle(gamemeta::fd::open(fd).and_then(|file| read_ids(file, &file_name, &mut files)))
    })
}

/// Strings of an ids handle, in `GmIdField` order: serial, game id, title id, CHD SHA-1, zip CRC, PSP title, PSP disc id,
/// Xbox id, RA md5, primary id (null where there is none).
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeIdsStrings(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jobjectArray {
    guard(std::ptr::null_mut(), || {
        if !is_pointer(handle) {
            return std::ptr::null_mut();
        }
        // SAFETY: the Kotlin side passes live handles only.
        let ids = unsafe { &*(handle as *const GmIds) }.ids();
        let values = [
            ids.serial.as_deref(),
            ids.game_id.as_deref(),
            ids.title_id.as_deref(),
            ids.chd_sha1.as_deref(),
            ids.zip_crc.as_deref(),
            ids.psp_title.as_deref(),
            ids.psp_disc_id.as_deref(),
            ids.xbox_id.as_deref(),
            ids.ra_md5.as_deref(),
            ids.primary_id(),
        ];
        let Ok(array) = env.new_object_array(values.len() as i32, "java/lang/String", JObject::null()) else {
            return std::ptr::null_mut();
        };
        for (i, v) in values.into_iter().enumerate() {
            if let Some(s) = v {
                let Ok(js) = env.new_string(s) else { return std::ptr::null_mut() };
                if env.set_object_array_element(&array, i as i32, &js).is_err() {
                    return std::ptr::null_mut();
                }
                let _ = env.delete_local_ref(js);
            }
        }
        array.into_raw()
    })
}

/// The zip ROM size of an ids handle, -1 when there is none.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeIdsZipSize(_env: JNIEnv, _class: JClass, handle: jlong) -> jlong {
    if !is_pointer(handle) {
        return -1;
    }
    // SAFETY: the Kotlin side passes live handles only.
    unsafe { &*(handle as *const GmIds) }.ids().zip_size.and_then(|s| jlong::try_from(s).ok()).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeIdsFree(_env: JNIEnv, _class: JClass, handle: jlong) {
    if is_pointer(handle) {
        // SAFETY: the Kotlin side frees each handle once, in a finally block.
        guard((), || drop(unsafe { Box::from_raw(handle as *mut GmIds) }));
    }
}

/// A streaming RA hasher for `file_name` of `size` bytes (-1 unknown): a handle, or 0 when the file is not hashed here.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeRaNew(
    mut env: JNIEnv,
    _class: JClass,
    file_name: JString,
    size: jlong,
) -> jlong {
    guard(0, || {
        let Some(file_name) = name(&mut env, &file_name) else { return 0 };
        RaHasher::for_file(&file_name, u64::try_from(size).ok())
            .map_or(0, |h| Box::into_raw(Box::new(Some(h))) as jlong)
    })
}

/// Feeds the first `length` bytes of `bytes` to the hasher.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeRaUpdate(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    bytes: JByteArray,
    length: jint,
) {
    guard((), || {
        if handle == 0 {
            return;
        }
        let Ok(bytes) = env.convert_byte_array(&bytes) else { return };
        let n = usize::try_from(length).unwrap_or(0).min(bytes.len());
        // SAFETY: the Kotlin side passes a live hasher handle (from nativeRaNew, not yet freed).
        if let Some(hasher) = unsafe { &mut *(handle as *mut Option<RaHasher>) } {
            hasher.update(&bytes[..n]);
        }
    });
}

/// The hasher's lower-case hex MD5, or null when the file is not hashable this way. Ends the hasher: later calls give null.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeRaFinish(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jni::sys::jstring {
    guard(std::ptr::null_mut(), || {
        if handle == 0 {
            return std::ptr::null_mut();
        }
        // SAFETY: as in `nativeRaUpdate`.
        let hash = unsafe { &mut *(handle as *mut Option<RaHasher>) }.take().and_then(RaHasher::finish);
        hash.and_then(|h| env.new_string(h).ok()).map_or(std::ptr::null_mut(), |s| s.into_raw())
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeRaFree(_env: JNIEnv, _class: JClass, handle: jlong) {
    if handle != 0 {
        // SAFETY: the Kotlin side frees each hasher once.
        guard((), || drop(unsafe { Box::from_raw(handle as *mut Option<RaHasher>) }));
    }
}

/// A disc id as numbers: status (0 ok, 1 too short, 2 bad magic, 3 unsupported container, 4 unreadable), region
/// (0 NTSC-U, 1 NTSC-J, 2 PAL, 3 NTSC-K, 4 unknown), system (0 GameCube, 1 Wii, 2 Xbox), then the id's ASCII bytes packed
/// big-endian into one number (at most 8).
fn disc_id_numbers(result: &DiscIdResult) -> [jlong; 4] {
    match result {
        DiscIdResult::Ok { id, region, system } => {
            let region = match region {
                DiscRegion::NtscU => 0,
                DiscRegion::NtscJ => 1,
                DiscRegion::Pal => 2,
                DiscRegion::NtscK => 3,
                DiscRegion::Unknown => 4,
            };
            let system = match system {
                DiscSystem::GameCube => 0,
                DiscSystem::Wii => 1,
                DiscSystem::Xbox => 2,
            };
            [0, region, system, id.bytes().take(8).fold(0, |acc, b| (acc << 8) | jlong::from(b))]
        }
        DiscIdResult::Refused(reason) => [
            match reason {
                DiscIdRefusal::TooShort => 1,
                DiscIdRefusal::BadMagic => 2,
                DiscIdRefusal::UnsupportedContainer => 3,
                DiscIdRefusal::Unreadable => 4,
            },
            4,
            0,
            0,
        ],
    }
}

fn numbers_array(env: &mut JNIEnv, numbers: &[jlong]) -> jlongArray {
    env.new_long_array(numbers.len() as i32).ok().map_or(std::ptr::null_mut(), |a| {
        if env.set_long_array_region(&a, 0, numbers).is_err() {
            return std::ptr::null_mut();
        }
        a.into_raw()
    })
}

/// The disc id of the header bytes (see [`disc_id_numbers`]).
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeDiscIdBytes(
    mut env: JNIEnv,
    _class: JClass,
    bytes: JByteArray,
) -> jlongArray {
    guard(std::ptr::null_mut(), || match env.convert_byte_array(&bytes) {
        Ok(bytes) => numbers_array(&mut env, &disc_id_numbers(&gamemeta_core::ids::disc_id(&bytes))),
        Err(_) => std::ptr::null_mut(),
    })
}

/// The disc id of the open descriptor `fd` (borrowed); an unusable descriptor throws `IOException`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeDiscIdFd(mut env: JNIEnv, _class: JClass, fd: jint) -> jlongArray {
    guard(std::ptr::null_mut(), || match gamemeta::fd::open(fd) {
        Ok(mut file) => numbers_array(&mut env, &disc_id_numbers(&gamemeta_core::ids::disc_id_of(&mut file))),
        Err(GmStatus::ErrIo | GmStatus::ErrArgument) => {
            let _ = env.throw_new("java/io/IOException", "game-metadata: the file could not be read");
            std::ptr::null_mut()
        }
        Err(_) => std::ptr::null_mut(),
    })
}

/// The loop length of a live scene handle in frames (60 to the second); 0 when still.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeRenderFrames(_env: JNIEnv, _class: JClass, handle: jlong) -> f32 {
    guard(0.0, || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(scene) = (unsafe { scene(handle) }) else { return 0.0 };
        // SAFETY: a live handle.
        unsafe { gamemeta::render::gm_render_frames(scene) }
    })
}

/// One CPU frame of a live scene handle at `frame`, `width` x `height` RGBA (top row first); null for a bad size.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeRenderSoft(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    frame: f32,
    width: jint,
    height: jint,
) -> jni::sys::jbyteArray {
    guard(std::ptr::null_mut(), || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(scene) = (unsafe { scene(handle) }) else { return std::ptr::null_mut() };
        let (Ok(w), Ok(h)) = (u32::try_from(width), u32::try_from(height)) else { return std::ptr::null_mut() };
        let pixels = u64::from(w) * u64::from(h);
        if w == 0 || h == 0 || pixels > banner3ds_soft::MAX_PIXELS {
            return std::ptr::null_mut();
        }
        let mut out = vec![0u8; (pixels * 4) as usize];
        if banner3ds_soft::render(scene.scene(), frame, w, h, &mut out).is_err() {
            return std::ptr::null_mut();
        }
        env.byte_array_from_slice(&out).map_or(std::ptr::null_mut(), |a| a.into_raw())
    })
}

/// A GL renderer and the library its functions come from.
struct JniGles {
    gles: *mut gamemeta::render::GmGles,
    #[cfg(unix)]
    library: *mut libc::c_void,
}

#[cfg(unix)]
unsafe extern "C" fn dlsym_load(
    library: *mut std::ffi::c_void,
    name: *const std::ffi::c_char,
) -> *const std::ffi::c_void {
    // SAFETY: `library` is a dlopen handle and `name` NUL-terminated (gm_gles_create's contract).
    unsafe { libc::dlsym(library, name) }.cast_const()
}

/// A GL renderer for a live scene handle in the context current on this thread, its functions from the shared library
/// `gl_library` (dlopen); 0 when the library, a function or a context is missing, or nothing could be set up.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeGlesCreate(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    gl_library: JString,
) -> jlong {
    guard(0, || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(scene) = (unsafe { scene(handle) }) else { return 0 };
        let Ok(name) = env.get_string(&gl_library).map(String::from) else { return 0 };
        create_gles(scene, &name)
    })
}

#[cfg(unix)]
fn create_gles(scene: &GmScene, library: &str) -> jlong {
    let Ok(name) = std::ffi::CString::new(library) else { return 0 };
    // SAFETY: a NUL-terminated path; dlopen has no other precondition.
    let lib = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if lib.is_null() {
        return 0;
    }
    let mut gles = std::ptr::null_mut();
    // SAFETY: `dlsym_load` returns the library's GL functions; the Kotlin side calls this with its context current.
    let status = unsafe { gamemeta::render::gm_gles_create(scene, Some(dlsym_load), lib, &mut gles) };
    if status != GmStatus::Ok {
        // SAFETY: from dlopen above, no function of it in use.
        unsafe { libc::dlclose(lib) };
        return 0;
    }
    Box::into_raw(Box::new(JniGles { gles, library: lib })) as jlong
}

#[cfg(not(unix))]
fn create_gles(_scene: &GmScene, _library: &str) -> jlong {
    0
}

/// Draws a GL renderer's frame into a viewport of the bound framebuffer (its context current).
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeGlesDraw(
    _env: JNIEnv,
    _class: JClass,
    gles: jlong,
    frame: f32,
    x: jint,
    y: jint,
    width: jint,
    height: jint,
) {
    if gles == 0 {
        return;
    }
    // SAFETY: the Kotlin side passes a live renderer from nativeGlesCreate, on its context's thread.
    guard((), || unsafe {
        gamemeta::render::gm_gles_draw((*(gles as *mut JniGles)).gles, frame, x, y, width, height);
    });
}

/// Deletes a GL renderer's resources (its context current) and frees it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeGlesDestroy(_env: JNIEnv, _class: JClass, gles: jlong) {
    if gles == 0 {
        return;
    }
    guard((), || {
        // SAFETY: the Kotlin side hands back a renderer from nativeGlesCreate, once, on its context's thread.
        let g = unsafe { Box::from_raw(gles as *mut JniGles) };
        // SAFETY: as above.
        unsafe { gamemeta::render::gm_gles_destroy(g.gles) };
        #[cfg(unix)]
        // SAFETY: the renderer using the library is gone.
        unsafe {
            libc::dlclose(g.library)
        };
    });
}

/// A Wii banner handle: a boxed `GmWiiBanner`, or `-GmStatus`.
fn into_wii_banner_handle(result: Result<GmWiiBanner, GmStatus>) -> jlong {
    match result {
        Ok(banner) => Box::into_raw(Box::new(banner)) as jlong,
        Err(status) => -(status as jlong),
    }
}

/// The handle's `GmWiiBanner`; `None` for a status.
///
/// Safety: a pointer `handle` came from `into_wii_banner_handle` and has not been freed.
unsafe fn wii_banner<'a>(handle: jlong) -> Option<&'a GmWiiBanner> {
    // SAFETY: caller contract.
    is_pointer(handle).then(|| unsafe { &*(handle as *const GmWiiBanner) })
}

/// The Wii channel banner of the RVZ, `opening.bnr` or `banner.bin` in `bytes` (a handle, or `-status`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeWiiBannerBytes(
    env: JNIEnv,
    _class: JClass,
    bytes: JByteArray,
) -> jlong {
    guard(-(GmStatus::ErrPanic as jlong), || match env.convert_byte_array(&bytes) {
        Ok(bytes) => into_wii_banner_handle(read_banner(SliceSource::new(&bytes))),
        Err(_) => -(GmStatus::ErrArgument as jlong),
    })
}

/// The Wii channel banner of the open descriptor `fd` (borrowed).
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeWiiBannerFd(_env: JNIEnv, _class: JClass, fd: jint) -> jlong {
    guard(-(GmStatus::ErrPanic as jlong), || into_wii_banner_handle(gamemeta::fd::banner_fd(fd)))
}

/// Counts of a Wii banner handle, in `GmWiiBannerCount` order: start frames, loop frames, window, text and bounding
/// panes skipped, missing textures.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeWiiBannerNumbers(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jlongArray {
    guard(std::ptr::null_mut(), || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(banner) = (unsafe { wii_banner(handle) }) else { return std::ptr::null_mut() };
        let whats = [
            GmWiiBannerCount::StartFrames,
            GmWiiBannerCount::LoopFrames,
            GmWiiBannerCount::SkippedWindows,
            GmWiiBannerCount::SkippedText,
            GmWiiBannerCount::SkippedBounds,
            GmWiiBannerCount::MissingTextures,
        ];
        let v: Vec<i64> = whats.iter().map(|&what| i64::from(banner.count(what as u32))).collect();
        let Ok(array) = env.new_long_array(v.len() as i32) else { return std::ptr::null_mut() };
        if env.set_long_array_region(&array, 0, &v).is_err() {
            return std::ptr::null_mut();
        }
        array.into_raw()
    })
}

/// The names of the textures a Wii banner's layout names and its archive lacks.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeWiiBannerMissing(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jobjectArray {
    guard(std::ptr::null_mut(), || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(banner) = (unsafe { wii_banner(handle) }) else { return std::ptr::null_mut() };
        let names = banner.banner().missing_textures();
        let Ok(array) = env.new_object_array(names.len() as i32, "java/lang/String", JObject::null()) else {
            return std::ptr::null_mut();
        };
        for (i, v) in names.iter().enumerate() {
            let Ok(js) = env.new_string(v) else { return std::ptr::null_mut() };
            if env.set_object_array_element(&array, i as i32, &js).is_err() {
                return std::ptr::null_mut();
            }
            let _ = env.delete_local_ref(js);
        }
        array.into_raw()
    })
}

/// The size `[width, height]` of a Wii banner frame at `scale`; null for a bad scale.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeWiiBannerSize(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    scale: f32,
) -> jni::sys::jintArray {
    guard(std::ptr::null_mut(), || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(banner) = (unsafe { wii_banner(handle) }) else { return std::ptr::null_mut() };
        let Some((w, h)) = banner.banner().pixel_size(scale) else { return std::ptr::null_mut() };
        let Ok(array) = env.new_int_array(2) else { return std::ptr::null_mut() };
        if env.set_int_array_region(&array, 0, &[w as i32, h as i32]).is_err() {
            return std::ptr::null_mut();
        }
        array.into_raw()
    })
}

/// One CPU frame of a Wii banner at `frame` and `scale`, RGBA (top row first); null for a bad scale.
#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeWiiBannerRender(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    frame: f32,
    scale: f32,
) -> jni::sys::jbyteArray {
    guard(std::ptr::null_mut(), || {
        // SAFETY: the Kotlin side passes live handles only.
        let Some(banner) = (unsafe { wii_banner(handle) }) else { return std::ptr::null_mut() };
        let Some(image) = banner.banner().render(frame, scale) else { return std::ptr::null_mut() };
        env.byte_array_from_slice(&image.pixels).map_or(std::ptr::null_mut(), |a| a.into_raw())
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_gamemeta_GameMeta_nativeWiiBannerFree(_env: JNIEnv, _class: JClass, handle: jlong) {
    if is_pointer(handle) {
        // SAFETY: the Kotlin side frees each handle once, in a finally block.
        guard((), || drop(unsafe { Box::from_raw(handle as *mut GmWiiBanner) }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_are_not_pointers() {
        for status in
            [GmStatus::NotFound, GmStatus::ErrArgument, GmStatus::ErrIo, GmStatus::ErrUnsupported, GmStatus::ErrPanic]
        {
            assert!(!is_pointer(into_handle(Err(status))));
        }
        assert!(!is_pointer(0));
    }

    #[test]
    fn tagged_arm64_pointers_are_pointers() {
        // An Android Scudo heap address: top-byte tag 0xb4, negative as a jlong.
        assert!(is_pointer(0xb400_0071_2345_6780_u64 as jlong));
        assert!(is_pointer(0x0000_7fff_1234_5670));
        assert!(is_pointer(0x8000_0000_u32 as jlong));
    }

    #[test]
    fn an_image_is_encoded_as_big_endian_size_then_pixels() {
        let image = RgbaImage { width: 0x0102, height: 3, pixels: vec![9; 4] };
        assert_eq!(encode_image(&image), [0, 0, 1, 2, 0, 0, 0, 3, 9, 9, 9, 9]);
    }

    #[test]
    fn a_handle_round_trips() {
        let handle =
            into_handle(Ok(
                gamemeta_core::read(&mut SliceSource::new(&gamemeta_fixtures::sample_3ds()), "s.3ds").unwrap()
            ));
        assert!(is_pointer(handle));
        // SAFETY: fresh handle, freed below.
        assert!(unsafe { info(handle) }.is_some());
        free(handle);
    }
}
