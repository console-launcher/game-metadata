# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Renders the Tux kart (SuperTuxKart, CC-BY-SA 3.0, see ../CREDITS.md) to the PNG frames the demo generator turns into icons
# and banners: a small z-buffered, textured, flat-shaded rasteriser (Python + numpy + Pillow; no GPU, no Blender), so the same
# mesh and textures give the same frames. Run by hand only when the art changes; the frames are committed in ./frames.
#   python3 -I render_tux.py <tux.json> <tux-textures-dir> <out-dir>
# <tux.json> is the kart posed as kart.xml's "straight" animation (frame 20) and exported from tux.blend by export_blend.py
# (parts with positions, normals, uvs, indices; x right, y up, z toward the viewer). Writes tux-<n>.png, 256x256 RGBA,
# n = 0..7: the kart turned n/8 of a turn about the vertical axis, starting at a three-quarter view from the front.
import json, math, sys
import numpy as np
from PIL import Image

mesh_path, tex_dir, out_dir = sys.argv[1:4]
mesh = json.load(open(mesh_path))["parts"]
TEXTURE = {"engine": "engine.png", "headlight_holder": "tuxkart.png"}
def texture_of(name):
    if name.startswith("tux."):
        return "tux_body.png"
    if name.startswith("Wheel") or name.startswith("axle"):
        return "wheel.png"
    return TEXTURE.get(name, "tuxkart.png")
textures = {}
def tex(name):
    if name not in textures:
        textures[name] = np.asarray(Image.open(f"{tex_dir}/{name}").convert("RGB"), dtype=np.float32) / 255.0
    return textures[name]

SS, SIZE = 3, 256
W = SIZE * SS
LIGHT = np.array([-0.35, 0.8, 0.5]); LIGHT /= np.linalg.norm(LIGHT)

def render(yaw_deg, pitch_deg=22.0):
    yaw, pitch = math.radians(yaw_deg), math.radians(pitch_deg)
    cy, sy, cp, sp = math.cos(yaw), math.sin(yaw), math.cos(pitch), math.sin(pitch)
    ry = np.array([[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]])
    rx = np.array([[1, 0, 0], [0, cp, -sp], [0, sp, cp]])
    m = rx @ ry
    color = np.zeros((W, W, 4), dtype=np.float32)
    depth = np.full((W, W), -1e9, dtype=np.float32)
    centre = np.array([0.0, 0.36, 0.0])
    scale = W / 1.7
    for name in sorted(mesh):
        p = mesh[name]
        pos = (np.array(p["positions"]).reshape(-1, 3) - centre) @ m.T
        nrm = np.array(p["normals"]).reshape(-1, 3) @ m.T
        uv = np.array(p["uvs"]).reshape(-1, 2)
        idx = np.array(p["indices"]).reshape(-1, 3)
        t = tex(texture_of(name))
        th, tw = t.shape[:2]
        sx = pos[:, 0] * scale + W / 2
        sy_ = -pos[:, 1] * scale + W / 2
        for a, b, c in idx:
            x0, x1, x2 = sx[[a, b, c]]; y0, y1, y2 = sy_[[a, b, c]]
            area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0)
            if abs(area) < 1e-6:
                continue
            lo_x, hi_x = int(max(min(x0, x1, x2), 0)), int(min(max(x0, x1, x2), W - 1))
            lo_y, hi_y = int(max(min(y0, y1, y2), 0)), int(min(max(y0, y1, y2), W - 1))
            if hi_x < lo_x or hi_y < lo_y:
                continue
            gx, gy = np.meshgrid(np.arange(lo_x, hi_x + 1) + 0.5, np.arange(lo_y, hi_y + 1) + 0.5)
            w0 = ((x1 - gx) * (y2 - gy) - (x2 - gx) * (y1 - gy)) / area
            w1 = ((x2 - gx) * (y0 - gy) - (x0 - gx) * (y2 - gy)) / area
            w2 = 1 - w0 - w1
            inside = (w0 >= 0) & (w1 >= 0) & (w2 >= 0)
            if not inside.any():
                continue
            z = w0 * pos[a, 2] + w1 * pos[b, 2] + w2 * pos[c, 2]
            sub = depth[lo_y:hi_y + 1, lo_x:hi_x + 1]
            win = inside & (z > sub)
            if not win.any():
                continue
            u = w0 * uv[a, 0] + w1 * uv[b, 0] + w2 * uv[c, 0]
            v = w0 * uv[a, 1] + w1 * uv[b, 1] + w2 * uv[c, 1]
            tx = np.clip((u % 1.0) * tw, 0, tw - 1).astype(int)
            ty = np.clip((1 - (v % 1.0)) * th, 0, th - 1).astype(int)
            n = w0[..., None] * nrm[a] + w1[..., None] * nrm[b] + w2[..., None] * nrm[c]
            n /= np.maximum(np.linalg.norm(n, axis=-1, keepdims=True), 1e-6)
            light = 0.62 + 0.45 * np.clip(n @ (LIGHT), 0, 1)
            rgb = np.clip(t[ty, tx] * light[..., None], 0, 1)
            sub[win] = z[win]
            region = color[lo_y:hi_y + 1, lo_x:hi_x + 1]
            region[win, :3] = rgb[win]
            region[win, 3] = 1
    img = Image.fromarray((color * 255 + 0.5).astype(np.uint8), "RGBA")
    # Premultiplied downscale so the edge pixels keep their colour.
    arr = np.asarray(img, dtype=np.float32) / 255.0
    arr[..., :3] *= arr[..., 3:]
    arr = arr.reshape(SIZE, SS, SIZE, SS, 4).mean(axis=(1, 3))
    a = arr[..., 3:]
    arr[..., :3] = np.where(a > 0, arr[..., :3] / np.maximum(a, 1e-6), 0)
    return Image.fromarray((np.clip(arr, 0, 1) * 255 + 0.5).astype(np.uint8), "RGBA")

for n in range(8):
    render(-30 + 45 * n).save(f"{out_dir}/tux-{n}.png", optimize=True)
