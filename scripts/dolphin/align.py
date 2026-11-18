# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Find which frame of a channel's icon animation each captured Dolphin tile frame shows, and how far apart they are.

    scripts/dolphin/align.py <slug> [--samples N] [--scale S] [--step K] [--write]

Needs numpy and Pillow (a developer tool; the reference test itself is Rust). Reads $WII_REFS/lib/<slug>/icon.bin and
$WII_REFS/out/<slug>/frames/NNNN.png, renders the icon with the `gamemeta` binary ($GAMEMETA, else target/release/gamemeta), and
for N captured frames (spread over the capture, after the tile settled) picks the icon frame with the smallest mean absolute
difference inside the tile's window. Prints `title tile_frame icon_frame mean max` per sample, titles and numbers only.
With --write the samples go to $WII_REFS/out/<slug>/samples.json (what `publish` carries to the library).
"""
import argparse, json, os, subprocess, sys, tempfile
import numpy as np
from PIL import Image

# The Wii Menu draws the icon in a window of the 235x130 tile crop; the window is centred on the layout's centre.
WINDOW = (28, 22, 200, 104)  # x0, y0, x1, y1 in the crop
SCALE = 1 / 0.84  # render pixels per layout unit that makes one render pixel one tile pixel


def root():
    return os.environ.get("WII_REFS") or os.path.expanduser("~/.local/share/dolphin-wii-refs")


def render_all(binary, icon, scale, frames, tmp):
    out = os.path.join(tmp, "r.png")
    args = [binary, "wii-banner", icon, "--scale", repr(scale), "-o", out]
    for f in frames:
        args += ["--frame", str(f)]
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL)
    return {f: os.path.join(tmp, "r-%d.png" % f) for f in frames}


def window_of(image, size):
    """The tile-sized window centred on the render's centre."""
    w, h = image.size
    tw, th = size
    x0, y0 = (w - tw) // 2, (h - th) // 2
    return np.asarray(image.convert("RGB").crop((x0, y0, x0 + tw, y0 + th)), dtype=np.float32)


def info(binary, icon):
    out = subprocess.run([binary, "wii-banner", icon], check=True, capture_output=True, text=True).stdout
    return json.loads(out.splitlines()[0])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("slug")
    ap.add_argument("--samples", type=int, default=5)
    ap.add_argument("--scale", type=float, default=SCALE)
    ap.add_argument("--step", type=int, default=10)
    ap.add_argument("--write", action="store_true")
    a = ap.parse_args()
    r = root()
    icon = os.path.join(r, "lib", a.slug, "icon.bin")
    frames_dir = os.path.join(r, "out", a.slug, "frames")
    binary = os.environ.get("GAMEMETA", "target/release/gamemeta")
    meta = info(binary, icon)
    length = meta["startFrames"] + meta["loopFrames"]
    names = sorted(os.listdir(frames_dir))
    have = [int(n[:-4]) for n in names]
    first, last = have[len(have) // 3], have[-1]
    picks = [first + (last - first) * i // max(a.samples - 1, 1) for i in range(a.samples)]
    size = (WINDOW[2] - WINDOW[0], WINDOW[3] - WINDOW[1])
    samples = []
    with tempfile.TemporaryDirectory() as tmp:
        coarse = list(range(0, length, a.step))
        paths = render_all(binary, icon, a.scale, coarse, tmp)
        wins = {f: window_of(Image.open(p), size) for f, p in paths.items()}
        for d in picks:
            tile = Image.open(os.path.join(frames_dir, "%04d.png" % d)).convert("RGB")
            t = np.asarray(tile.crop(WINDOW), dtype=np.float32)
            scores = {f: float(np.abs(w - t).mean()) for f, w in wins.items()}
            best = min(scores, key=scores.get)
            fine = [f % length for f in range(best - a.step + 1, best + a.step)]
            fpaths = render_all(binary, icon, a.scale, fine, tmp)
            fine_scores = {}
            for f, p in fpaths.items():
                diff = np.abs(window_of(Image.open(p), size) - t)
                fine_scores[f] = (float(diff.mean()), float(diff.max()))
            f = min(fine_scores, key=lambda k: fine_scores[k][0])
            samples.append({"tile_frame": d, "icon_frame": f, "mean": round(fine_scores[f][0], 2), "max": round(fine_scores[f][1], 1)})
            print(a.slug, d, f, samples[-1]["mean"], samples[-1]["max"])
    if a.write:
        with open(os.path.join(r, "out", a.slug, "samples.json"), "w") as fh:
            json.dump({"scale": a.scale, "window": WINDOW, "samples": samples}, fh, indent=1)
        os.chmod(os.path.join(r, "out", a.slug, "samples.json"), 0o600)


if __name__ == "__main__":
    sys.exit(main())
