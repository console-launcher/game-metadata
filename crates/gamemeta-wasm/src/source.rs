// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A [`ByteSource`] over a JS `Blob` (a picked `File`), read in place with `FileReaderSync` (Web Worker only).

use gamemeta_core::ids::Opener;
use gamemeta_core::{ByteSource, ReadError};
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/js/io.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    fn read_range(blob: &JsValue, offset: f64, length: u32) -> Result<Uint8Array, JsValue>;
    fn blob_size(blob: &JsValue) -> f64;
}

/// Random access to one blob: each `read_at` slices just those bytes.
#[derive(Debug)]
pub struct BlobSource {
    blob: JsValue,
    size: u64,
}

impl BlobSource {
    pub fn new(blob: JsValue) -> BlobSource {
        let size = blob_size(&blob);
        BlobSource { blob, size: if size.is_finite() && size > 0.0 { size as u64 } else { 0 } }
    }
}

impl ByteSource for BlobSource {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        if buf.is_empty() {
            return Ok(());
        }
        let end = offset.checked_add(buf.len() as u64).ok_or(ReadError::OutOfRange)?;
        if end > self.size {
            return Err(ReadError::OutOfRange);
        }
        let bytes = read_range(&self.blob, offset as f64, buf.len() as u32).map_err(|_| ReadError::Io)?;
        if bytes.length() as usize != buf.len() {
            return Err(ReadError::Io);
        }
        bytes.copy_to(buf);
        Ok(())
    }
}

/// Opens the files beside the one being read (a cue sheet's tracks) by asking a JS function for a blob by path.
#[derive(Debug)]
pub struct JsOpener<'a>(pub Option<&'a js_sys::Function>);

impl Opener for JsOpener<'_> {
    fn open(&mut self, path: &str) -> Option<Box<dyn ByteSource + '_>> {
        let blob = self.0?.call1(&JsValue::NULL, &JsValue::from_str(path)).ok()?;
        (!blob.is_undefined() && !blob.is_null()).then(|| Box::new(BlobSource::new(blob)) as Box<dyn ByteSource>)
    }
}
