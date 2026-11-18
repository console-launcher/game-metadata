#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Reference keyframes from one HOME Menu frame dump (the emulator's lossless two-screen composite, 60 fps, emulated time).

  frames.py <dump.mkv> <out-dir> <banner.cbmd> <version> <clips> [<clip-seconds>]

- top screen = the top 400 x 240 of the composite (internal scale 1)
- banner start: the frame after which the distance from the pre-move scene (the first lit frames) stays past half way to the
  banner's own level (the median over the dump's second half) to the end of the dump, stepped back to where its fade began
- c<clip>.png per clip in <clips> ("300 340 ..."): the top screen <clip> refreshes after the banner start (vsync below),
  everything outside HOME's banner window and the status bars painted pure magenta, so only the banner's pixels remain
- clip.mp4 when <clip-seconds> > 0: that many seconds from the first clip, masked the same way, H.264 30 fps, small
- manifest.json: emulator version, date, banner start, per keyframe the clip and dump frame, the mask, the banner's sha256
Nothing of the HOME Menu itself is kept: the mask removes the wall and status bars from every saved frame."""
import hashlib
import json
import os
import subprocess
import sys
from datetime import date

import numpy as np

W, H = 400, 240
CX, CY, RX, RY, INSET = 200.0, 131.75, 196.0, 103.9, 3.0
ROW_FROM, ROW_TO = 24, 216
MAGENTA = np.array([255, 0, 255], np.uint8)
yy, xx = np.mgrid[0:H, 0:W].astype(np.float32) + 0.5
INSIDE = (((xx - CX) / (RX - INSET)) ** 2 + ((yy - CY) / (RY - INSET)) ** 2 <= 1.0) & (yy >= ROW_FROM) & (yy < ROW_TO)


def vsync(n):
    """HOME's banner clock: refreshes since frame 0 of the dump (it drops about 3 in 1066 frames)."""
    return n - (3 * n + 533) // 1066


def run(*a, **k):
    return subprocess.run(a, check=True, **k)


def masked(rgb):
    rgb = rgb.copy()
    rgb[~INSIDE] = MAGENTA
    return rgb


def start_frame(dump):
    raw = run('ffmpeg', '-v', 'error', '-i', dump, '-vf', f'crop={W}:{H}:0:0,scale=100:60:flags=area,format=gray', '-f',
              'rawvideo', '-', capture_output=True).stdout
    g = np.frombuffer(raw, np.uint8).reshape(-1, 60, 100)[:, 6:54].reshape(-1, 48 * 100).astype(np.float32)
    lit = np.nonzero(g.mean(axis=1) > 60)[0]
    k = int(lit[0]) + 10 if len(lit) else 0
    base = np.median(g[k:k + 20], axis=0)
    d = np.abs(g - base).mean(axis=1)
    d[:k] = 0
    pre, post = float(np.median(d[k:k + 20])), float(np.median(d[len(d) // 2:]))
    if post - pre < 4:
        sys.exit('no banner in the dump (the top screen never changed)')
    thr = (pre + post) / 2
    # the last return to the pre-move level ends HOME's own transition (icons fly past before a banner loads); the banner
    # holds past the threshold from there to the end of the dump
    below = np.nonzero(d[k:] <= thr)[0]
    i = k + int(below[-1]) + 1 if len(below) else k
    if i < len(d) - 60:
        # the crossing is a few frames into the banner's fade; step back to where the change began
        j = i
        while j > k and d[j - 1] > pre + (thr - pre) / 4:
            j -= 1
        return j, len(g)
    sys.exit('no stable banner start found')


def frame_of(start, clip):
    return next(n for n in range(start, start + 4 * clip + 600) if vsync(n) - vsync(start) >= clip)


def read_frames(dump, first, count):
    raw = run('ffmpeg', '-v', 'error', '-i', dump, '-vf',
              f'crop={W}:{H}:0:0,select=between(n\\,{first}\\,{first + count - 1})', '-vsync', '0', '-f', 'rawvideo',
              '-pix_fmt', 'rgb24', '-', capture_output=True).stdout
    return np.frombuffer(raw, np.uint8).reshape(-1, H, W, 3)


def main():
    dump, out, banner, version, clips = sys.argv[1:6]
    seconds = float(sys.argv[6]) if len(sys.argv) > 6 else 0.0
    clips = [int(c) for c in clips.split()]
    from PIL import Image
    os.makedirs(out, exist_ok=True)
    start, total = start_frame(dump)
    picks = [(c, frame_of(start, c)) for c in clips]
    if picks[-1][1] >= total:
        sys.exit(f'dump too short: {total} frames, clip {clips[-1]} needs frame {picks[-1][1]}')
    for c, n in picks:
        Image.fromarray(masked(read_frames(dump, n, 1)[0])).save(os.path.join(out, f'c{c:04d}.png'), optimize=True)
    if seconds > 0:
        first = picks[0][1]
        frames = read_frames(dump, first, int(seconds * 60))[::2]
        enc = subprocess.Popen(['ffmpeg', '-v', 'error', '-y', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-s', f'{W}x{H}', '-r', '30',
                                '-i', '-', '-c:v', 'libx264', '-crf', '26', '-preset', 'veryslow', '-pix_fmt', 'yuv420p',
                                '-movflags', '+faststart', '-an', os.path.join(out, 'clip.mp4')], stdin=subprocess.PIPE)
        for f in frames:
            enc.stdin.write(masked(f).tobytes())
        enc.stdin.close()
        if enc.wait():
            sys.exit('clip encode failed')
    manifest = {
        'emulator_version': version, 'captured': date.today().isoformat(), 'internal_scale': 1, 'size': [W, H],
        'banner_sha256': hashlib.sha256(open(banner, 'rb').read()).hexdigest(),
        'banner_start_frame': start, 'dump_fps': 60,
        'frames': [{'clip': c, 'dump_frame': n, 'file': f'c{c:04d}.png'} for c, n in picks],
        'clip_note': 'c<clip>.png = the top screen <clip> refreshes after the banner appeared (vsync: n - (3n + 533) / 1066)',
        'mask': 'pure magenta outside HOME\'s banner window (ellipse centre 200,131.75, radii 193x100.9) and rows 24..215',
    }
    if seconds > 0:
        manifest['video'] = {'file': 'clip.mp4', 'from_clip': clips[0], 'seconds': seconds, 'fps': 30}
    json.dump(manifest, open(os.path.join(out, 'manifest.json'), 'w'), indent=1, sort_keys=True)
    print(f'{out}: start {start}, {len(picks)} keyframes' + (', clip.mp4' if seconds > 0 else ''))


if __name__ == '__main__':
    if len(sys.argv) < 6:
        sys.exit(__doc__)
    main()
