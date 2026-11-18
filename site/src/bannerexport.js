// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// What the page needs to offer "Export as": the formats, the file name, the progress words and whether this browser can make
// an MP4. Pure (no DOM; the browser's codec classes are passed in), so `node --test` covers it.

export const FORMATS = [
  { id: 'gif', label: 'GIF', ext: 'gif', mime: 'image/gif' },
  { id: 'mp4', label: 'MP4 video', ext: 'mp4', mime: 'video/mp4' },
  { id: 'webp', label: 'WebP (animated)', ext: 'webp', mime: 'image/webp' },
];

// H.264 profiles and levels tried in turn: main 4.0, baseline 3.1, high 4.0.
export const H264 = ['avc1.4d0028', 'avc1.42001f', 'avc1.640028'];

// "<title>-banner.<ext>": the title with what a file name cannot hold taken out.
export function exportFileName(title, ext) {
  const clean = String(title ?? '')
    .replace(/[\u0000-\u001f\u007f\\/:*?"<>|]+/g, ' ')
    .replace(/\s+/g, ' ')
    .replace(/^[.\s]+|[.\s]+$/g, '')
    .slice(0, 80)
    .trim();
  return `${clean || 'game'}-banner.${ext}`;
}

export function formatBytes(n) {
  return n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`;
}

// The first H.264 config this browser can encode at `width` x `height`, or null (no WebCodecs, or no H.264 encoder).
export async function pickH264(g, width, height) {
  if (typeof g.VideoEncoder !== 'function' || typeof g.VideoFrame !== 'function') return null;
  for (const codec of H264) {
    const config = { codec, width, height, bitrate: Math.round(width * height * 60 * 0.25), framerate: 60, avc: { format: 'avc' } };
    try {
      if ((await g.VideoEncoder.isConfigSupported(config))?.supported) return config;
    } catch {
      // try the next
    }
  }
  return null;
}

// -> { ok: true } or { ok: false, reason }
export async function probeMp4(g = globalThis) {
  if (await pickH264(g, 400, 240)) return { ok: true };
  return { ok: false, reason: 'This browser can’t make video files.' };
}

// "Making GIF… 42%"
export const progressText = (label, done, total) => `Making ${label}… ${total ? Math.min(99, Math.floor((done / total) * 100)) : 0}%`;
