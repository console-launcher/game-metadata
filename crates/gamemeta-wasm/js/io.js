// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Synchronous random access to a Blob (a File the visitor picked), for a Web Worker: FileReaderSync over Blob.slice reads
// just the bytes asked for, so a multi-gigabyte ROM is never loaded whole.
let reader = null;

export function read_range(blob, offset, length) {
  reader ??= new FileReaderSync();
  return new Uint8Array(reader.readAsArrayBuffer(blob.slice(offset, offset + length)));
}

export function blob_size(blob) {
  return blob.size;
}
