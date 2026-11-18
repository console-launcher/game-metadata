# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Blender (4.x/5.x, background) script: SuperTuxKart's Tux kart source (tux.blend) to the banner's mesh file (text).
#   blender -b tux.blend --python export_blend.py -- <out.mesh> [frame]
# Poses the armature at `frame` (20, the kart's "straight" pose in kart.xml), evaluates every mesh, and writes each part as
# triangles in banner space: x right, y up, z toward the viewer (Blender's -x, z, y: the kart faces the camera). Tux's body
# is split by the bone that weighs most on each triangle, so the banner can move his head and arms as rigid parts.
# Output is rounded to 1e-4 and ordered by name, so the same .blend gives the same bytes.
import bpy, bmesh, sys

argv = sys.argv[sys.argv.index("--") + 1:]
out_path = argv[0]
frame = int(argv[1]) if len(argv) > 1 else 20
bpy.context.scene.frame_set(frame)
deps = bpy.context.evaluated_depsgraph_get()
SKIP = {"christmas_hat", "headlight", "headlight.001"}

def conv(v):
    return (-v[0], v[2], v[1])

def r(x):
    return round(x, 4) + 0.0

arm = bpy.data.objects.get("Armature")
pose_heads = {b.name: conv(arm.matrix_world @ b.head) for b in arm.pose.bones} if arm else {}
parts = {}
for o in sorted(bpy.data.objects, key=lambda o: o.name):
    if o.type != "MESH" or o.name in SKIP:
        continue
    ev = o.evaluated_get(deps)
    me = ev.to_mesh()
    bm = bmesh.new(); bm.from_mesh(me); bmesh.ops.triangulate(bm, faces=bm.faces[:]); bm.to_mesh(me); bm.free()
    me.calc_loop_triangles() if hasattr(me, "calc_loop_triangles") else None
    uv = me.uv_layers.active.data if me.uv_layers.active else None
    mw = ev.matrix_world
    nm = mw.to_3x3().inverted().transposed()
    src = o.data  # vertex groups live on the original mesh (same vertex order: armature modifiers keep it)
    groups = {g.index: g.name for g in o.vertex_groups}
    for poly in me.polygons:
        key = o.name
        if o.name == "tux" and groups:
            score = {}
            for vi in poly.vertices:
                for g in src.vertices[vi].groups:
                    score[groups[g.group]] = score.get(groups[g.group], 0.0) + g.weight
            key = "tux." + max(sorted(score), key=lambda k: score[k]) if score else "tux.Body"
        p = parts.setdefault(key, {"verts": {}, "positions": [], "normals": [], "uvs": [], "indices": []})
        for li in poly.loop_indices:
            loop = me.loops[li]
            pos = conv(mw @ me.vertices[loop.vertex_index].co)
            n = (nm @ (loop.normal if poly.use_smooth else poly.normal)).normalized()
            nrm = conv(n)
            t = uv[li].uv if uv else (0.0, 0.0)
            k = tuple(r(x) for x in (*pos, *nrm, t[0], t[1]))
            idx = p["verts"].get(k)
            if idx is None:
                idx = len(p["verts"]); p["verts"][k] = idx
                p["positions"] += [k[0], k[1], k[2]]; p["normals"] += [k[3], k[4], k[5]]; p["uvs"] += [k[6], k[7]]
            p["indices"].append(idx)
    ev.to_mesh_clear()

origins = {}
for name in ("WheelFront.L", "WheelFront.R", "WheelRear.L", "WheelRear.R"):
    if name in bpy.data.objects:
        origins[name] = [r(x) for x in conv(bpy.data.objects[name].matrix_world.translation)]
for name, head in pose_heads.items():
    origins["tux." + name] = [r(x) for x in head]
# Text out, one record per line: "part <name> <verts> <tris>", then "v x y z nx ny nz u v" per vertex and "t a b c" per
# triangle; "origin <name> x y z" per pivot (wheel centres, Tux's posed bone heads); "nitro x y z" per exhaust.
lines = []
for k, v in sorted(parts.items()):
    n = len(v["positions"]) // 3
    lines.append(f"part {k} {n} {len(v['indices']) // 3}")
    for i in range(n):
        vals = v["positions"][3 * i:3 * i + 3] + v["normals"][3 * i:3 * i + 3] + v["uvs"][2 * i:2 * i + 2]
        lines.append("v " + " ".join(f"{x:.4f}" for x in vals))
    idx = v["indices"]
    for i in range(0, len(idx), 3):
        lines.append(f"t {idx[i]} {idx[i + 1]} {idx[i + 2]}")
for name, xyz in sorted(origins.items()):
    lines.append(f"origin {name} " + " ".join(f"{x:.4f}" for x in xyz))
for n in ("nitro1", "nitro2"):
    if n in bpy.data.objects:
        lines.append("nitro " + " ".join(f"{r(x):.4f}" for x in conv(bpy.data.objects[n].matrix_world.translation)))
with open(out_path, "w") as f:
    f.write("\n".join(lines) + "\n")
for k, v in sorted(parts.items()):
    print("PART", k, len(v["positions"]) // 3, "verts", len(v["indices"]) // 3, "tris")
