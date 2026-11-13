#!/usr/bin/env python3
"""Builds every model and texture of the "Ashen Keep" example game from code.

    python3 tools/make_ashen_assets.py

Output goes to crates/cryo-editor/template/ashen/ and is embedded in the editor.
Characters are rigged glTF files (skeleton, smooth skin weights, animation clips and
textures); scenery is OBJ with tiling textures. Nothing here comes from an external asset: change the numbers and run again.
"""
import base64, json, math, os, struct
import numpy as np
from PIL import Image

OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "cryo-editor", "template", "ashen")
os.makedirs(OUT, exist_ok=True)

# ------------------------------------------------------------------ small math
def rot(rx, ry, rz):
    """Rotation matrix for Euler angles in degrees, applied as Ry * Rx * Rz."""
    x, y, z = (math.radians(a) for a in (rx, ry, rz))
    mx = np.array([[1, 0, 0], [0, math.cos(x), -math.sin(x)], [0, math.sin(x), math.cos(x)]])
    my = np.array([[math.cos(y), 0, math.sin(y)], [0, 1, 0], [-math.sin(y), 0, math.cos(y)]])
    mz = np.array([[math.cos(z), -math.sin(z), 0], [math.sin(z), math.cos(z), 0], [0, 0, 1]])
    return my @ mx @ mz

def quat(rx, ry, rz):
    m = rot(rx, ry, rz)
    t = m[0, 0] + m[1, 1] + m[2, 2]
    if t > 0:
        s = math.sqrt(t + 1) * 2
        q = [(m[2, 1] - m[1, 2]) / s, (m[0, 2] - m[2, 0]) / s, (m[1, 0] - m[0, 1]) / s, s / 4]
    elif m[0, 0] > m[1, 1] and m[0, 0] > m[2, 2]:
        s = math.sqrt(1 + m[0, 0] - m[1, 1] - m[2, 2]) * 2
        q = [s / 4, (m[0, 1] + m[1, 0]) / s, (m[0, 2] + m[2, 0]) / s, (m[2, 1] - m[1, 2]) / s]
    elif m[1, 1] > m[2, 2]:
        s = math.sqrt(1 + m[1, 1] - m[0, 0] - m[2, 2]) * 2
        q = [(m[0, 1] + m[1, 0]) / s, s / 4, (m[1, 2] + m[2, 1]) / s, (m[0, 2] - m[2, 0]) / s]
    else:
        s = math.sqrt(1 + m[2, 2] - m[0, 0] - m[1, 1]) * 2
        q = [(m[0, 2] + m[2, 0]) / s, (m[1, 2] + m[2, 1]) / s, s / 4, (m[1, 0] - m[0, 1]) / s]
    n = math.sqrt(sum(c * c for c in q))
    return [c / n for c in q]

# ------------------------------------------------------------------ mesh builder
def skin_of(joint, y):
    """A vertex follows one joint, or two blended: (joint, other joint, weight of the other)."""
    if callable(joint):
        return joint(y)
    return joint if isinstance(joint, tuple) else (joint, joint, 0.0)

def bend(upper, lower, y_joint, width=0.06):
    """Skin for a limb that hangs down: `upper` above the joint, `lower` below, blended around it."""
    return lambda y: (upper, lower, float(np.clip((y_joint + width - y) / (2 * width), 0, 1)))

def spline(points, steps=4):
    """Catmull-Rom through (y, radius) points: turns a rough profile into a smooth one."""
    pts = [points[0]] + list(points) + [points[-1]]
    out = []
    for i in range(1, len(pts) - 2):
        p0, p1, p2, p3 = (np.array(p, float) for p in pts[i - 1:i + 3])
        for s in range(steps):
            t = s / steps
            out.append(tuple(0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t + (-p0 + 3 * p1 - 3 * p2 + p3) * t ** 3)))
    out.append(tuple(points[-1]))
    return [(y, max(r, 0.0)) for y, r in out]

TEX = 0.3  # character textures repeat every 30 cm

class Mesh:
    """Triangles grouped by material. A vertex is (position, normal, skin, uv)."""
    def __init__(self):
        self.tris = {}

    def tri(self, mat, a, b, c):
        self.tris.setdefault(mat, []).extend([a, b, c])

    def quad(self, mat, a, b, c, d):
        self.tri(mat, a, b, c)
        self.tri(mat, a, c, d)

    def box(self, mat, center, size, joint=0, r=None):
        c = np.array(center, float)
        h = np.array(size, float) / 2
        m = rot(*r) if r else np.eye(3)
        sk = skin_of(joint, c[1])
        faces = [((1, 0, 0), [(1, -1, -1), (1, 1, -1), (1, 1, 1), (1, -1, 1)]), ((-1, 0, 0), [(-1, -1, 1), (-1, 1, 1), (-1, 1, -1), (-1, -1, -1)]),
                 ((0, 1, 0), [(-1, 1, -1), (-1, 1, 1), (1, 1, 1), (1, 1, -1)]), ((0, -1, 0), [(-1, -1, 1), (-1, -1, -1), (1, -1, -1), (1, -1, 1)]),
                 ((0, 0, 1), [(1, -1, 1), (1, 1, 1), (-1, 1, 1), (-1, -1, 1)]), ((0, 0, -1), [(-1, -1, -1), (-1, 1, -1), (1, 1, -1), (1, -1, -1)])]
        for n, cs in faces:
            axis = int(np.argmax(np.abs(n)))
            vs = []
            for s in cs:
                local = h * np.array(s)
                uv = [(local[2], local[1]), (local[0], local[2]), (local[0], local[1])][axis]
                vs.append((c + m @ local, m @ np.array(n, float), sk, (uv[0] / TEX, uv[1] / TEX)))
            self.quad(mat, *vs)

    def lathe(self, mat, profile, center=(0, 0), sides=14, sx=1.0, sz=1.0, joint=0, flat=False, r=None, pivot=None, arc=(0.0, 1.0)):
        """Profile is a list of (y, radius) from bottom to top, spun around a vertical axis.
        `arc` keeps only part of the turn (0.75 is the front, -Z), for visors and plates."""
        cx, cz = center
        m = rot(*r) if r else np.eye(3)
        pv = np.array(pivot if pivot else (cx, profile[0][0], cz), float)
        whole = arc == (0.0, 1.0)
        mean_r = max(sum(p[1] for p in profile) / len(profile), 1e-3)
        ucount = max(1, round(math.tau * mean_r * (arc[1] - arc[0]) / TEX))
        def vert(i, k):
            y, rad = profile[i]
            f = k / sides
            a = (arc[0] + (arc[1] - arc[0]) * f) * math.tau
            p = np.array([cx + math.cos(a) * rad * sx, y, cz + math.sin(a) * rad * sz])
            j0, j1 = max(i - 1, 0), min(i + 1, len(profile) - 1)
            dy, dr = profile[j1][0] - profile[j0][0], profile[j1][1] - profile[j0][1]
            n = np.array([math.cos(a) * dy / sx, -dr, math.sin(a) * dy / sz])
            l = np.linalg.norm(n)
            n = n / l if l > 1e-9 else np.array([0, 1.0 if i else -1.0, 0])
            return (pv + m @ (p - pv), m @ n, skin_of(joint, y), (f * ucount, y / TEX))
        for i in range(len(profile) - 1):
            for k in range(sides):
                a, b, c, d = vert(i, k), vert(i + 1, k), vert(i + 1, k + 1), vert(i, k + 1)
                if flat:
                    fn = np.cross(b[0] - a[0], d[0] - a[0])
                    fn = fn / (np.linalg.norm(fn) or 1)
                    a, b, c, d = ((v[0], fn, v[2], v[3]) for v in (a, b, c, d))
                self.quad(mat, a, b, c, d)
        if whole:
            for i, up in ((0, -1.0), (len(profile) - 1, 1.0)):
                if profile[i][1] > 1e-6:
                    y = profile[i][0]
                    n = m @ np.array([0, up, 0])
                    mid = (pv + m @ (np.array([cx, y, cz]) - pv), n, skin_of(joint, y), (0.5, 0.5))
                    for k in range(sides):
                        a, b = vert(i, k), vert(i, k + 1)
                        a, b = (a[0], n, a[2], a[3]), (b[0], n, b[2], b[3])
                        self.tri(mat, mid, b if up > 0 else a, a if up > 0 else b)

    def tube(self, mat, pts, radius, sides=6, joint=0, flatten=1.0):
        """A round bar swept along a path: ribs, horns, bows, bones."""
        pts = [np.array(p, float) for p in pts]
        radii = radius if isinstance(radius, (list, tuple)) else [radius] * len(pts)
        rings, length = [], 0.0
        for i, p in enumerate(pts):
            t = pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]
            t = t / (np.linalg.norm(t) or 1)
            up = np.array([1.0, 0, 0]) if abs(t[1]) > 0.9 else np.array([0, 1.0, 0])
            u = np.cross(t, up)
            u = u / np.linalg.norm(u)
            v = np.cross(t, u)
            if i:
                length += np.linalg.norm(p - pts[i - 1])
            ring = []
            for k in range(sides + 1):
                a = k / sides * math.tau
                d = math.cos(a) * u + math.sin(a) * v * flatten
                ring.append((p + d * radii[i], d / (np.linalg.norm(d) or 1), skin_of(joint, p[1]), (k / sides, length / TEX)))
            rings.append(ring)
        for i in range(len(rings) - 1):
            for k in range(sides):
                self.quad(mat, rings[i][k], rings[i + 1][k], rings[i + 1][k + 1], rings[i][k + 1])
        for i, sign in ((0, -1.0), (len(pts) - 1, 1.0)):
            if radii[i] > 1e-4:
                t = (pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]) * sign
                n = t / (np.linalg.norm(t) or 1)
                mid = (pts[i], n, skin_of(joint, pts[i][1]), (0.5, 0.5))
                for k in range(sides):
                    a, b = rings[i][k], rings[i][k + 1]
                    self.tri(mat, mid, (a[0], n, a[2], a[3]), (b[0], n, b[2], b[3]))

def ball(y, r, n=7, squash=1.0):
    return [(y + math.cos(math.pi * (1 - i / n)) * r * squash, max(math.sin(math.pi * i / n) * r, 0.0)) for i in range(n + 1)]

# Surface finish per material name: (roughness 0 = mirror .. 1 = chalk, metallic 0 .. 1).
PBR = {
    "plate": (0.42, 0.85), "trim": (0.38, 1.0), "blade": (0.25, 1.0), "mail": (0.5, 0.85), "rusty": (0.72, 0.35),
    "cloth": (0.92, 0.0), "rag": (0.95, 0.0), "leather": (0.68, 0.0), "bone": (0.62, 0.0), "wood": (0.78, 0.0),
    "dark": (0.6, 0.3), "black": (0.7, 0.0), "stone": (0.88, 0.0), "cobble": (0.8, 0.0), "flag": (0.55, 0.0),
    "grass": (0.95, 0.0), "roof": (0.7, 0.0), "iron": (0.5, 0.8), "gold": (0.35, 1.0), "rock": (0.9, 0.0),
    "bark": (0.85, 0.0), "leaf": (0.8, 0.0), "leaf2": (0.8, 0.0), "glass": (0.08, 0.0),
}

def normal_for(tex):
    """The normal map that goes with a colour texture, when the generator wrote one."""
    if not tex:
        return None
    n = tex.replace(".jpg", "_n.jpg").replace(".png", "_n.png")
    return n if os.path.exists(os.path.join(OUT, n)) else None

# ------------------------------------------------------------------ OBJ output (scenery)
def write_obj(name, mesh, materials, tile=2.0):
    """`materials`: name -> (rgb, texture file or None). Texture coordinates repeat every `tile` metres."""
    lines, vi = [f"# Ashen Keep: {name} (generated by tools/make_ashen_assets.py)", f"mtllib {name}.mtl"], 1
    for mat, verts in mesh.tris.items():
        lines.append(f"usemtl {mat}")
        for i in range(0, len(verts), 3):
            face = []
            for p, n, _, _ in verts[i:i + 3]:
                axis = int(np.argmax(np.abs(n)))
                u, v = [(p[2], p[1]), (p[0], p[2]), (p[0], p[1])][axis]
                lines.append(f"v {p[0]:.4f} {p[1]:.4f} {p[2]:.4f}")
                lines.append(f"vt {u / tile:.4f} {v / tile:.4f}")
                lines.append(f"vn {n[0]:.3f} {n[1]:.3f} {n[2]:.3f}")
                face.append(f"{vi}/{vi}/{vi}")
                vi += 1
            lines.append("f " + " ".join(face))
    open(os.path.join(OUT, name + ".obj"), "w").write("\n".join(lines) + "\n")
    mtl = []
    for mat in mesh.tris:
        rgb, tex, *extra = materials[mat]
        mtl += [f"newmtl {mat}", f"Kd {rgb[0]} {rgb[1]} {rgb[2]}"] + ([f"map_Kd {tex}"] if tex else [])
        if len(extra) > 0 and extra[0]:
            mtl.append("Ke {} {} {}".format(*extra[0]))
        if len(extra) > 1 and extra[1] < 1.0:
            mtl.append(f"d {extra[1]}")
        if normal_for(tex):
            mtl.append(f"norm {normal_for(tex)}")
        if mat in PBR:
            mtl += [f"Pr {PBR[mat][0]}", f"Pm {PBR[mat][1]}"]
    open(os.path.join(OUT, name + ".mtl"), "w").write("\n".join(mtl) + "\n")

# ------------------------------------------------------------------ glTF output (characters)
def write_gltf(name, mesh, materials, joints, clips):
    """`materials`: name -> (rgb, texture file or None). `joints`: list of (name, parent or None, position).
    `clips`: name -> (duration, tracks); tracks maps a joint to [(time, (rx, ry, rz))] and "move" to
    [(time, (x, y, z))] for the root. Equal vertices are shared, so the files stay small."""
    bin_, views, accessors = bytearray(), [], []
    def add(data, fmt, kind, count, ctype, extra=None):
        while len(bin_) % 4:
            bin_.append(0)
        raw = struct.pack("<%d%s" % (len(data), fmt), *data)
        views.append({"buffer": 0, "byteOffset": len(bin_), "byteLength": len(raw)})
        bin_.extend(raw)
        acc = {"bufferView": len(views) - 1, "componentType": ctype, "count": count, "type": kind}
        acc.update(extra or {})
        accessors.append(acc)
        return len(accessors) - 1
    prims, mats, images, textures = [], [], [], []
    for mat, verts in mesh.tris.items():
        seen, pos, nor, uvs, jnt, wgt, idx = {}, [], [], [], [], [], []
        for p, n, sk, uv in verts:
            key = (tuple(np.round(p, 4)), tuple(np.round(n, 2)), sk[0], sk[1], round(sk[2], 2), round(uv[0], 3), round(uv[1], 3))
            if key not in seen:
                seen[key] = len(seen)
                pos += list(p)
                nor += list(n / (np.linalg.norm(n) or 1))
                uvs += [uv[0], uv[1]]
                jnt += [sk[0], sk[1], 0, 0]
                wgt += [1.0 - sk[2], sk[2], 0.0, 0.0]
            idx.append(seen[key])
        count = len(seen)
        P = np.array(pos).reshape(-1, 3)
        prims.append({"attributes": {
            "POSITION": add(pos, "f", "VEC3", count, 5126, {"min": P.min(0).tolist(), "max": P.max(0).tolist()}),
            "NORMAL": add(nor, "f", "VEC3", count, 5126), "TEXCOORD_0": add(uvs, "f", "VEC2", count, 5126),
            "JOINTS_0": add(jnt, "H", "VEC4", count, 5123), "WEIGHTS_0": add(wgt, "f", "VEC4", count, 5126)},
            "indices": add(idx, "I", "SCALAR", len(idx), 5125), "material": len(mats)})
        rgb, tex, *extra = materials[mat]
        rough, metal = PBR.get(mat, (0.7, 0.0))
        opacity = extra[1] if len(extra) > 1 else 1.0
        pbr = {"baseColorFactor": list(rgb) + [opacity], "roughnessFactor": rough, "metallicFactor": metal}
        def image(uri):
            if {"uri": uri} not in images:
                images.append({"uri": uri})
                textures.append({"source": len(images) - 1})
            return images.index({"uri": uri})
        material = {"name": mat, "pbrMetallicRoughness": pbr}
        if len(extra) > 0 and extra[0]:
            material["emissiveFactor"] = list(extra[0])
        if opacity < 1.0:
            material["alphaMode"] = "BLEND"
        if tex:
            pbr["baseColorTexture"] = {"index": image(tex)}
            if normal_for(tex):
                material["normalTexture"] = {"index": image(normal_for(tex))}
        mats.append(material)
    ibm = []
    for _, _, p in joints:
        ibm += [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, -p[0], -p[1], -p[2], 1]
    ibm_acc = add(ibm, "f", "MAT4", len(joints), 5126)
    nodes = [{"name": name, "mesh": 0, "skin": 0}]
    for i, (jn, parent, p) in enumerate(joints):
        base = joints[parent][2] if parent is not None else (0, 0, 0)
        node = {"name": jn, "translation": [p[0] - base[0], p[1] - base[1], p[2] - base[2]]}
        kids = [k + 1 for k, j in enumerate(joints) if j[1] == i]
        if kids:
            node["children"] = kids
        nodes.append(node)
    animations = []
    for clip, (duration, tracks) in clips.items():
        samplers, channels = [], []
        for j in range(len(joints)):   # every joint gets a key at 0 and at the end, so clips blend cleanly
            keys = sorted(tracks.get(j, [(0, (0, 0, 0))]))
            if keys[0][0] > 0:
                keys.insert(0, (0, keys[0][1]))
            if keys[-1][0] < duration:
                keys.append((duration, keys[-1][1]))
            t = add([k[0] for k in keys], "f", "SCALAR", len(keys), 5126, {"min": [0], "max": [duration]})
            v = add([c for k in keys for c in quat(*k[1])], "f", "VEC4", len(keys), 5126)
            samplers.append({"input": t, "output": v, "interpolation": "LINEAR"})
            channels.append({"sampler": len(samplers) - 1, "target": {"node": j + 1, "path": "rotation"}})
        move = sorted(tracks.get("move", [(0, (0, 0, 0))]))
        if move[-1][0] < duration:
            move.append((duration, move[-1][1]))
        root = joints[0][2]
        t = add([k[0] for k in move], "f", "SCALAR", len(move), 5126, {"min": [0], "max": [duration]})
        v = add([root[i] + k[1][i] for k in move for i in range(3)], "f", "VEC3", len(move), 5126)
        samplers.append({"input": t, "output": v, "interpolation": "LINEAR"})
        channels.append({"sampler": len(samplers) - 1, "target": {"node": 1, "path": "translation"}})
        for key, keys in tracks.items():
            if isinstance(key, tuple) and key[0] == "move":
                j = key[1]
                keys = sorted(keys)
                if keys[0][0] > 0:
                    keys.insert(0, (0, (0, 0, 0)))
                if keys[-1][0] < duration:
                    keys.append((duration, keys[-1][1]))
                p, parent = joints[j][2], joints[j][1]
                base = [p[i] - (joints[parent][2][i] if parent is not None else 0) for i in range(3)]
                t = add([k[0] for k in keys], "f", "SCALAR", len(keys), 5126, {"min": [0], "max": [duration]})
                v = add([base[i] + k[1][i] for k in keys for i in range(3)], "f", "VEC3", len(keys), 5126)
                samplers.append({"input": t, "output": v, "interpolation": "LINEAR"})
                channels.append({"sampler": len(samplers) - 1, "target": {"node": j + 1, "path": "translation"}})
        animations.append({"name": clip, "samplers": samplers, "channels": channels})
    doc = {"asset": {"version": "2.0", "generator": "Cryo Engine (make_ashen_assets.py)"}, "scene": 0,
           "scenes": [{"nodes": [0, 1]}], "nodes": nodes, "meshes": [{"primitives": prims}], "materials": mats,
           "skins": [{"joints": list(range(1, len(joints) + 1)), "inverseBindMatrices": ibm_acc}],
           "animations": animations, "accessors": accessors, "bufferViews": views,
           "buffers": [{"byteLength": len(bin_), "uri": "data:application/octet-stream;base64," + base64.b64encode(bytes(bin_)).decode()}]}
    if images:
        doc["images"], doc["textures"] = images, textures
    json.dump(doc, open(os.path.join(OUT, name + ".gltf"), "w"), separators=(",", ":"))
    return sum(len(v) for v in mesh.tris.values()) // 3

# ------------------------------------------------------------------ characters
# Joints. The character faces -Z (the engine's "forward"); +X is its right.
HIPS, CHEST, HEAD, ARM_L, FORE_L, ARM_R, FORE_R, LEG_L, SHIN_L, LEG_R, SHIN_R = range(11)
JOINTS = [("Hips", None, (0, 0.95, 0)), ("Chest", HIPS, (0, 1.12, 0)), ("Head", CHEST, (0, 1.5, 0)),
          ("ArmL", CHEST, (-0.27, 1.42, 0)), ("ForeL", ARM_L, (-0.27, 1.14, 0)),
          ("ArmR", CHEST, (0.27, 1.42, 0)), ("ForeR", ARM_R, (0.27, 1.14, 0)),
          ("LegL", HIPS, (-0.11, 0.92, 0)), ("ShinL", LEG_L, (-0.11, 0.5, 0)),
          ("LegR", HIPS, (0.11, 0.92, 0)), ("ShinR", LEG_R, (0.11, 0.5, 0))]
FRONT = 0.75   # lathe angle of the character's front
SIDES = ((-1, ARM_L, FORE_L, LEG_L, SHIN_L), (1, ARM_R, FORE_R, LEG_R, SHIN_R))

def waist(y):   # the trunk bends between hips and chest
    return (HIPS, CHEST, float(np.clip((y - 0.98) / 0.16, 0, 1)))

def armoured(m, boss):
    """A knight in plate over mail. `boss` makes the darker, heavier Warden."""
    plate, trim = "plate", "trim"
    # mail under everything: one continuous, bending surface per limb and for the trunk
    m.lathe("mail", spline([(0.84, 0.13), (0.92, 0.165), (1.0, 0.155), (1.1, 0.17), (1.25, 0.2), (1.38, 0.2), (1.46, 0.14), (1.5, 0.07)]), sides=18, sx=1.15, sz=0.78, joint=waist)
    for side, arm, fore, leg, shin in SIDES:
        ax, lx = 0.27 * side, 0.11 * side
        m.lathe("mail", spline([(0.9, 0.042), (1.0, 0.052), (1.14, 0.05), (1.3, 0.058), (1.43, 0.052)]), center=(ax, 0), sides=12, joint=bend(arm, fore, 1.14))
        m.lathe("leather", spline([(0.08, 0.05), (0.3, 0.068), (0.5, 0.064), (0.72, 0.086), (0.93, 0.09)]), center=(lx, 0), sides=12, joint=bend(leg, shin, 0.5))
        # arm harness: pauldron with lames, rerebrace, couter, vambrace, gauntlet
        big = 1.25 if boss else (1.3 if side < 0 else 0.82)   # the shield side carries the heavy pauldron
        m.lathe(plate, ball(1.45, 0.125 * big, squash=0.75), center=(ax * 1.08, 0), sides=14, joint=arm)
        for i in range(3):
            m.lathe(plate, [(1.39 - i * 0.045, 0.118 * big - i * 0.012), (1.43 - i * 0.045, 0.128 * big - i * 0.012)], center=(ax * 1.07, 0), sides=14, joint=arm)
        m.lathe(trim, [(1.385, 0.13 * big), (1.4, 0.131 * big)], center=(ax * 1.07, 0), sides=14, joint=arm)
        m.lathe(plate, spline([(1.19, 0.062), (1.26, 0.07), (1.33, 0.068)]), center=(ax, 0), sides=12, joint=arm)
        m.lathe(plate, ball(1.14, 0.068, squash=0.9), center=(ax, 0.012), sides=10, joint=(arm, fore, 0.5))
        m.lathe(plate, spline([(0.92, 0.05), (0.99, 0.064), (1.09, 0.06)]), center=(ax, 0), sides=12, joint=fore)
        m.lathe(plate, [(0.89, 0.072), (0.95, 0.056)], center=(ax, 0), sides=12, joint=fore)
        m.lathe("leather", ball(0.845, 0.052), center=(ax, 0), sides=10, sx=0.8, joint=fore)
        if boss:
            m.lathe(trim, [(1.5, 0.035), (1.72, 0.0)], center=(ax * 1.15, 0), sides=6, joint=arm, r=(0, 0, -28 * side), pivot=(ax * 1.15, 1.5, 0))
        # leg harness: cuisse, poleyn, greave, sabaton
        m.lathe(plate, spline([(0.57, 0.086), (0.7, 0.1), (0.86, 0.104)]), center=(lx, 0), sides=12, joint=leg, arc=(FRONT - 0.33, FRONT + 0.33))
        m.lathe(plate, ball(0.5, 0.078, squash=0.95), center=(lx, -0.022), sides=10, joint=(leg, shin, 0.5))
        m.lathe(trim, [(0.44, 0.082), (0.455, 0.082)], center=(lx, -0.01), sides=10, joint=shin)
        m.lathe(plate, spline([(0.11, 0.058), (0.26, 0.08), (0.44, 0.07)]), center=(lx, 0), sides=12, joint=shin)
        m.tube(plate, [(lx, 0.06, 0.05), (lx, 0.058, -0.05), (lx, 0.045, -0.15), (lx, 0.03, -0.22)], [0.055, 0.06, 0.05, 0.02], sides=10, joint=shin)
        m.box("dark", (lx, 0.012, -0.05), (0.1, 0.024, 0.27), shin)
    # cuirass, fauld of overlapping hoops, gorget
    m.lathe(plate, spline([(1.03, 0.168), (1.1, 0.188), (1.25, 0.218), (1.38, 0.222), (1.45, 0.17), (1.485, 0.095)]), sides=18, sx=1.15, sz=0.84, joint=CHEST)
    m.tube(trim, [(0, 1.44, -0.165), (0, 1.3, -0.19), (0, 1.1, -0.165)], 0.012, sides=5, joint=CHEST)      # ridge down the breastplate
    for i in range(3):
        m.lathe(plate, [(0.9 - i * 0.065, 0.186 + i * 0.012), (0.99 - i * 0.065, 0.172 + i * 0.012)], sides=18, sx=1.15, sz=0.86, joint=HIPS)
    m.lathe("leather", [(0.995, 0.178), (1.035, 0.178)], sides=18, sx=1.15, sz=0.86, joint=HIPS)
    m.box(trim, (0, 1.015, -0.156), (0.07, 0.05, 0.02), HIPS)
    m.lathe(plate, spline([(1.455, 0.115), (1.5, 0.088), (1.535, 0.094)]), sides=14, joint=CHEST)
    # tabard and helmet
    for z in (-0.158, 0.158):
        m.box("cloth", (0, 0.66, z), (0.22, 0.5, 0.012), HIPS)
        m.box(trim, (0, 0.415, z), (0.22, 0.02, 0.014), HIPS)
    m.lathe(plate, spline([(1.5, 0.086), (1.54, 0.118), (1.6, 0.13), (1.68, 0.128), (1.75, 0.105), (1.8, 0.058), (1.815, 0.0)]), sides=18, sz=1.12, joint=HEAD)
    m.lathe(plate, spline([(1.545, 0.126), (1.6, 0.146), (1.655, 0.138)]), sides=12, sz=1.14, joint=HEAD, arc=(FRONT - 0.21, FRONT + 0.21))   # visor
    m.box("black", (0, 1.645, -0.153), (0.17, 0.018, 0.02), HEAD)                         # eye slit
    for x in (-0.05, -0.025, 0.025, 0.05):
        m.box("black", (x, 1.585, -0.158), (0.008, 0.04, 0.012), HEAD)                    # breaths
    m.lathe(trim, [(1.535, 0.121), (1.552, 0.123)], sides=18, sz=1.12, joint=HEAD)
    m.tube(trim, [(0, 1.7, -0.13), (0, 1.8, -0.06), (0, 1.823, 0.01), (0, 1.76, 0.13)], 0.011, sides=5, joint=HEAD)   # crest
    if boss:
        for side in (-1, 1):                                                             # horns
            m.tube("bone", [(0.1 * side, 1.72, 0), (0.2 * side, 1.78, -0.01), (0.27 * side, 1.9, -0.04), (0.25 * side, 2.04, -0.09)], [0.04, 0.034, 0.024, 0.004], sides=8, joint=HEAD)
        for i in range(7):                                                               # torn cape, in strips
            x = -0.24 + i * 0.08
            long = 0.75 + 0.18 * math.sin(i * 2.4)
            m.tube("cloth", [(x, 1.44, 0.17), (x, 1.2, 0.23), (x, 1.44 - long * 0.7, 0.27 + 0.03 * math.sin(i)), (x, 1.44 - long, 0.24)], [0.045, 0.045, 0.04, 0.012], sides=4, joint=waist, flatten=0.2)
        m.tube(trim, [(-0.27, 1.47, 0.12), (0, 1.5, 0.17), (0.27, 1.47, 0.12)], 0.02, sides=6, joint=CHEST)
    else:
        m.lathe("cloth", spline([(1.44, 0.2), (1.5, 0.15), (1.56, 0.13), (1.6, 0.145)]), sides=14, sx=1.1, sz=0.95, joint=CHEST)   # cowl
        for i in range(6):                                                               # travel-worn cloak, torn into strips
            x = -0.2 + i * 0.08
            long = 0.62 + 0.2 * math.sin(i * 1.9 + 0.5)
            m.tube("cloth", [(x, 1.46, 0.15), (x, 1.22, 0.21), (x, 1.46 - long * 0.7, 0.24 + 0.03 * math.sin(i * 2.2)), (x, 1.46 - long, 0.21)], [0.045, 0.045, 0.04, 0.01], sides=4, joint=waist, flatten=0.2)

def sword(m, joint, long, wide, blade, trim, guard=0.24):
    hx, hy = 0.27, 0.85
    m.tube("leather", [(hx, hy, 0.07), (hx, hy, -0.12)], 0.017, sides=8, joint=joint)
    m.lathe(trim, ball(hy, 0.03), center=(hx, 0.095), sides=8, joint=joint)
    m.tube(trim, [(hx - guard / 2, hy, -0.13), (hx, hy, -0.145), (hx + guard / 2, hy, -0.13)], [0.012, 0.02, 0.012], sides=6, joint=joint)
    # a diamond-section blade: a four-sided lathe laid along -Z, with a fuller down the middle
    m.lathe(blade, [(hy, wide * 0.5), (hy + long * 0.8, wide * 0.42), (hy + long, 0.0)], center=(hx, -0.15), sides=4, sz=0.16, flat=True, joint=joint, r=(-90, 0, 0), pivot=(hx, hy, -0.15))
    m.box("dark", (hx, hy, -0.15 - long * 0.35), (wide * 0.22, wide * 0.17, long * 0.6), joint)

def skeleton(m, style):
    b = "bone"
    # spine and ribcage
    y = 0.9
    while y < 1.5:
        m.lathe(b, [(y, 0.016), (y + 0.01, 0.028), (y + 0.03, 0.028), (y + 0.04, 0.016)], center=(0, 0.06), sides=8, joint=waist)
        y += 0.045
    for i in range(7):
        ry = 1.42 - i * 0.045
        rx = 0.115 + 0.05 * math.sin((i + 1.2) / 7.5 * math.pi)
        end = 262 - max(i - 4, 0) * 28          # the lowest ribs do not reach the breastbone
        for side in (-1, 1):
            pts = []
            for s in range(9):
                a = math.radians(95 + (end - 95) * s / 8)
                pts.append((-math.cos(a) * rx * side, ry - 0.06 * s / 8, math.sin(a) * 0.105 + 0.02))
            m.tube(b, pts, 0.0105, sides=6, joint=CHEST)
    m.box(b, (0, 1.27, -0.088), (0.04, 0.26, 0.016), CHEST)                               # breastbone
    for side in (-1, 1):
        m.tube(b, [(0.02 * side, 1.415, -0.085), (0.14 * side, 1.44, -0.04), (0.25 * side, 1.43, 0.0)], 0.012, sides=6, joint=CHEST)   # collarbone
        m.tube(b, [(0.12 * side, 1.4, 0.1), (0.2 * side, 1.34, 0.11), (0.15 * side, 1.25, 0.105), (0.12 * side, 1.4, 0.1)], 0.014, sides=5, joint=CHEST, flatten=0.4)  # shoulder blade
        # pelvis: a crest sweeping from the spine round to the front
        m.tube(b, [(0.02 * side, 0.98, 0.07), (0.13 * side, 1.01, 0.04), (0.16 * side, 0.95, -0.04), (0.08 * side, 0.87, -0.08), (0.015 * side, 0.86, -0.07)], [0.025, 0.03, 0.028, 0.022, 0.018], sides=7, joint=HIPS)
    m.lathe(b, [(0.86, 0.02), (0.9, 0.05), (0.96, 0.04)], center=(0, 0.06), sides=8, joint=HIPS)   # sacrum
    # skull
    m.lathe(b, spline([(1.555, 0.045), (1.585, 0.086), (1.64, 0.104), (1.7, 0.1), (1.75, 0.07), (1.778, 0.0)]), center=(0, 0.012), sides=16, sz=1.18, joint=HEAD)
    m.box(b, (0, 1.572, -0.072), (0.105, 0.045, 0.075), HEAD)                              # upper jaw
    m.tube(b, [(-0.078, 1.6, 0.03), (-0.072, 1.535, -0.01), (-0.045, 1.512, -0.1), (0.045, 1.512, -0.1), (0.072, 1.535, -0.01), (0.078, 1.6, 0.03)], 0.015, sides=6, joint=HEAD)  # jaw
    for i in range(7):
        x = -0.042 + i * 0.014
        m.box(b, (x, 1.542, -0.108 + abs(x) * 0.25), (0.011, 0.022, 0.012), HEAD)          # teeth
    for side in (-1, 1):
        m.lathe("black", ball(1.635, 0.03), center=(0.042 * side, -0.094), sides=8, joint=HEAD)  # eye sockets
        m.tube(b, [(0.03 * side, 1.672, -0.105), (0.075 * side, 1.668, -0.085), (0.097 * side, 1.635, -0.04)], 0.011, sides=5, joint=HEAD)  # brow
        m.tube(b, [(0.03 * side, 1.6, -0.1), (0.085 * side, 1.6, -0.07), (0.1 * side, 1.61, 0.0)], 0.012, sides=5, joint=HEAD)             # cheekbone
    m.box("black", (0, 1.6, -0.108), (0.022, 0.035, 0.02), HEAD)                           # nose
    for side, arm, fore, leg, shin in SIDES:
        ax, lx = 0.27 * side, 0.11 * side
        m.lathe(b, spline([(1.14, 0.03), (1.17, 0.017), (1.38, 0.018), (1.425, 0.034)]), center=(ax, 0), sides=8, joint=arm)      # humerus
        m.tube(b, [(ax - 0.015, 1.14, 0), (ax - 0.02, 1.0, 0), (ax - 0.012, 0.9, 0)], [0.016, 0.012, 0.016], sides=6, joint=fore)  # radius
        m.tube(b, [(ax + 0.015, 1.14, 0.005), (ax + 0.02, 1.0, 0.005), (ax + 0.012, 0.9, 0.005)], [0.016, 0.011, 0.014], sides=6, joint=fore)  # ulna
        m.box(b, (ax, 0.87, 0), (0.06, 0.05, 0.03), fore)                                  # palm
        for f in range(4):                                                                # fingers curl round the grip
            fx = ax - 0.024 + f * 0.016
            m.tube(b, [(fx, 0.85, -0.005), (fx, 0.815, -0.02), (fx, 0.8, 0.0)], 0.006, sides=4, joint=fore)
        m.lathe(b, spline([(0.5, 0.036), (0.54, 0.022), (0.86, 0.022), (0.9, 0.04), (0.93, 0.03)]), center=(lx, 0), sides=8, joint=leg)   # femur
        m.lathe(b, ball(0.5, 0.03), center=(lx, -0.035), sides=7, joint=(leg, shin, 0.5))    # kneecap
        m.tube(b, [(lx - 0.008, 0.5, 0), (lx - 0.008, 0.3, -0.005), (lx - 0.005, 0.08, 0)], [0.028, 0.017, 0.022], sides=7, joint=shin)   # shin bone
        m.tube(b, [(lx + 0.028, 0.48, 0.01), (lx + 0.03, 0.1, 0.01)], 0.009, sides=5, joint=shin)
        m.box(b, (lx, 0.045, 0.03), (0.05, 0.06, 0.07), shin)                              # heel
        for f in range(4):                                                                # foot bones
            fx = lx - 0.027 + f * 0.018
            m.tube(b, [(fx, 0.06, 0.0), (fx, 0.03, -0.1), (fx, 0.012, -0.17 + abs(f - 1.5) * 0.015)], [0.01, 0.009, 0.006], sides=4, joint=shin)
    # remains of war gear
    if style == "brute":
        m.lathe("rusty", spline([(1.05, 0.19), (1.2, 0.215), (1.38, 0.215), (1.45, 0.16)]), sides=16, sx=1.15, sz=0.85, joint=CHEST, arc=(FRONT - 0.36, FRONT + 0.36))
        m.lathe("rusty", ball(1.46, 0.14, squash=0.8), center=(0.3, 0), sides=12, joint=ARM_R)
        m.lathe("rusty", [(1.5, 0.04), (1.7, 0.0)], center=(0.33, 0), sides=6, joint=ARM_R)
        m.lathe("rusty", spline([(1.66, 0.112), (1.72, 0.1), (1.77, 0.06), (1.79, 0.0)]), center=(0, 0.012), sides=14, sz=1.18, joint=HEAD)
        for side in (-1, 1):
            m.tube(b, [(0.1 * side, 1.7, 0.01), (0.2 * side, 1.72, 0.0), (0.26 * side, 1.82, -0.03), (0.23 * side, 1.93, -0.06)], [0.035, 0.03, 0.02, 0.004], sides=7, joint=HEAD)
        for i in range(5):
            m.box("rag", (-0.1 + i * 0.05, 0.72 - 0.03 * (i % 2), -0.1), (0.045, 0.34 - 0.06 * (i % 2), 0.008), HIPS)
    else:
        m.lathe("rusty", spline([(1.66, 0.108), (1.72, 0.1), (1.765, 0.07), (1.79, 0.0)]), center=(0, 0.012), sides=14, sz=1.18, joint=HEAD)   # dented cap
        m.lathe("rusty", [(1.655, 0.125), (1.67, 0.11)], center=(0, 0.012), sides=14, sz=1.18, joint=HEAD)
        for i in range(4):
            m.box("rag", (-0.075 + i * 0.05, 0.74 - 0.04 * (i % 2), -0.095), (0.045, 0.3 - 0.08 * (i % 2), 0.008), HIPS)
    hx, hy = 0.27, 0.85
    if style == "archer":
        pts = [(-0.27, hy + 0.55 * math.sin(a), -0.06 + 0.22 * (1 - math.cos(a))) for a in np.linspace(-1.25, 1.25, 11)]
        m.tube("wood", pts, [0.008 + 0.012 * math.cos(a) for a in np.linspace(-1.25, 1.25, 11)], sides=6, joint=FORE_L)   # bow
        m.tube("black", [pts[0], pts[-1]], 0.003, sides=3, joint=FORE_L)
        m.lathe("leather", [(1.0, 0.045), (1.45, 0.055)], center=(0.08, 0.15), sides=8, joint=CHEST, r=(0, 0, -18), pivot=(0.08, 1.2, 0.15))   # quiver
        for i in range(4):
            m.tube("wood", [(0.14 + i * 0.012, 1.42, 0.14 + (i % 2) * 0.02), (0.17 + i * 0.012, 1.6, 0.14 + (i % 2) * 0.02)], 0.005, sides=4, joint=CHEST)
    elif style == "brute":
        m.tube("wood", [(hx, hy, 0.12), (hx, hy, -0.95)], 0.02, sides=8, joint=FORE_R)       # axe haft
        for s in range(8):                                                                # crescent blade
            a0, a1 = math.radians(-70 + s * 17.5), math.radians(-70 + (s + 1) * 17.5)
            mid = (a0 + a1) / 2
            m.box("rusty", (hx, hy + 0.2 + 0.1 * math.cos(mid), -0.82 + 0.3 * math.sin(mid)), (0.014, 0.25 + 0.14 * math.cos(mid), 0.098), FORE_R, r=(-math.degrees(mid) * 0.35, 0, 0))
        m.lathe("rusty", [(hy - 0.02, 0.035), (hy + 0.04, 0.035)], center=(hx, -0.82), sides=8, joint=FORE_R)
    else:
        sword(m, FORE_R, 0.82, 0.05, "rusty", "rusty", 0.2)

def humanoid(style):
    m = Mesh()
    if style in ("knight", "boss"):
        armoured(m, style == "boss")
        if style == "boss":
            sword(m, FORE_R, 1.55, 0.09, "blade", "trim", 0.4)
        else:
            sword(m, FORE_R, 0.95, 0.055, "blade", "trim")
            # round shield on the left arm: domed face, iron rim and boss, a painted cross
            pivot = (-0.36, 1.0, -0.03)
            turn = dict(joint=FORE_L, r=(0, 0, 90), pivot=pivot)
            m.lathe("cloth", [(0.98, 0.0), (0.98, 0.27), (0.995, 0.27), (1.015, 0.15), (1.02, 0.0)], center=(-0.36, -0.03), sides=20, sx=1.1, sz=0.9, **turn)
            m.lathe("plate", [(0.976, 0.262), (0.98, 0.288), (1.004, 0.288), (1.0, 0.262)], center=(-0.36, -0.03), sides=20, sx=1.1, sz=0.9, **turn)
            m.lathe("trim", ball(1.03, 0.055, squash=0.7), center=(-0.36, -0.03), sides=10, **turn)
            m.box("trim", (-0.377, 1.0, -0.03), (0.008, 0.46, 0.05), FORE_L)
            m.box("trim", (-0.377, 1.03, -0.03), (0.008, 0.05, 0.36), FORE_L)
    else:
        skeleton(m, style)
    return m

def clips_for(style):
    I, R = (0, 0, 0), None
    c = {}
    # standing: the sword arm slightly raised, a slow breath
    guard = {ARM_R: [(0, (25, 0, 8))], FORE_R: [(0, (35, 0, 0))], ARM_L: [(0, (10, 0, -8))], FORE_L: [(0, (30, 0, 0))]}
    c["Idle"] = (2.0, {**guard, CHEST: [(0, (-2, 0, 0)), (1.0, (2, 0, 0)), (2.0, (-2, 0, 0))],
                       HEAD: [(0, (0, -4, 0)), (1.0, (0, 4, 0)), (2.0, (0, -4, 0))],
                       "move": [(0, (0, 0, 0)), (1.0, (0, -0.015, 0)), (2.0, (0, 0, 0))]})
    if style in ("skeleton", "archer", "brute"):
        c["Idle"][1][HIPS] = [(0, (-12, 0, 0))]
        c["Idle"][1][HEAD] = [(0, (14, -8, 0)), (1.0, (14, 8, 0)), (2.0, (14, -8, 0))]
        c["Idle"][1][ARM_R] = [(0, (8, 0, 14))]
    def gait(d, swing, lean, bob):
        q = d / 4
        return (d, {HIPS: [(0, (-lean, 0, 0))], CHEST: [(0, (-lean * 0.6, 8, 0)), (2 * q, (-lean * 0.6, -8, 0)), (d, (-lean * 0.6, 8, 0))],
                    LEG_L: [(0, (swing, 0, 0)), (2 * q, (-swing * 0.8, 0, 0)), (d, (swing, 0, 0))],
                    LEG_R: [(0, (-swing * 0.8, 0, 0)), (2 * q, (swing, 0, 0)), (d, (-swing * 0.8, 0, 0))],
                    SHIN_L: [(0, (-10, 0, 0)), (q, (-swing * 1.3, 0, 0)), (2 * q, (-25, 0, 0)), (3 * q, (-5, 0, 0)), (d, (-10, 0, 0))],
                    SHIN_R: [(0, (-25, 0, 0)), (q, (-5, 0, 0)), (2 * q, (-10, 0, 0)), (3 * q, (-swing * 1.3, 0, 0)), (d, (-25, 0, 0))],
                    ARM_L: [(0, (-swing * 0.6, 0, -8)), (2 * q, (swing * 0.6, 0, -8)), (d, (-swing * 0.6, 0, -8))],
                    FORE_L: [(0, (45, 0, 0))],
                    ARM_R: [(0, (30, 0, 8)), (2 * q, (15, 0, 8)), (d, (30, 0, 8))], FORE_R: [(0, (40, 0, 0))],
                    "move": [(0, (0, 0, 0)), (q, (0, bob, 0)), (2 * q, (0, 0, 0)), (3 * q, (0, bob, 0)), (d, (0, 0, 0))]})
    c["Run"] = gait(0.56, 42, 12, 0.05)
    c["Walk"] = gait(0.95, 24, 5, 0.025)
    def swing(d, wind, hit, chest_w, chest_h, rest=guard):
        """One cut: wind up, strike at 40% of the clip, recover."""
        a, b = d * 0.28, d * 0.45
        return (d, {ARM_R: [(0, rest[ARM_R][0][1]), (a, wind), (b, hit), (d, rest[ARM_R][0][1])],
                    FORE_R: [(0, (35, 0, 0)), (a, (70, 0, 0)), (b, (5, 0, 0)), (d, (35, 0, 0))],
                    CHEST: [(0, I), (a, chest_w), (b, chest_h), (d, I)],
                    ARM_L: [(0, (10, 0, -8)), (a, (-15, 0, -25)), (b, (25, 0, -12)), (d, (10, 0, -8))], FORE_L: [(0, (30, 0, 0))],
                    LEG_L: [(0, I), (b, (22, 0, 0)), (d, I)], LEG_R: [(0, I), (b, (-18, 0, 0)), (d, I)],
                    SHIN_R: [(0, I), (b, (-20, 0, 0)), (d, I)],
                    "move": [(0, (0, 0, 0)), (b, (0, -0.05, 0)), (d, (0, 0, 0))]})
    c["Attack1"] = swing(0.5, (120, -50, 30), (20, 60, 10), (0, -30, 0), (-12, 35, 0))      # right to left
    c["Attack2"] = swing(0.5, (60, 70, 10), (40, -70, 40), (0, 30, 0), (-8, -40, 0))         # backhand
    c["Attack3"] = swing(0.7, (190, 0, 10), (-10, 0, 5), (14, 0, 0), (-28, 0, 0))            # overhead
    c["Heavy"] = swing(1.0, (150, -80, 40), (10, 90, 0), (0, -55, 0), (-15, 60, 0))
    c["Block"] = (0.3, {ARM_L: [(0, (10, 0, -8)), (0.12, (75, 40, -20))], FORE_L: [(0, (30, 0, 0)), (0.12, (70, 0, 0))],
                        ARM_R: [(0, (25, 0, 8))], FORE_R: [(0, (35, 0, 0))], CHEST: [(0, I), (0.12, (-6, -18, 0))],
                        LEG_L: [(0, I), (0.12, (15, 0, 0))], SHIN_L: [(0, I), (0.12, (-20, 0, 0))]})
    tuck = (95, 0, 0)
    c["Roll"] = (0.55, {HIPS: [(0, I), (0.14, (-90, 0, 0)), (0.28, (-180, 0, 0)), (0.42, (-270, 0, 0)), (0.55, (-360, 0, 0))],
                        CHEST: [(0, I), (0.1, (-35, 0, 0)), (0.45, (-35, 0, 0)), (0.55, I)],
                        LEG_L: [(0, I), (0.1, tuck), (0.45, tuck), (0.55, I)], LEG_R: [(0, I), (0.1, tuck), (0.45, tuck), (0.55, I)],
                        SHIN_L: [(0, I), (0.1, (-110, 0, 0)), (0.45, (-110, 0, 0)), (0.55, I)],
                        SHIN_R: [(0, I), (0.1, (-110, 0, 0)), (0.45, (-110, 0, 0)), (0.55, I)],
                        ARM_L: [(0, (10, 0, -8)), (0.1, (60, 0, -10)), (0.55, (10, 0, -8))], ARM_R: [(0, (25, 0, 8)), (0.1, (60, 0, 10)), (0.55, (25, 0, 8))],
                        "move": [(0, (0, 0, 0)), (0.12, (0, -0.45, 0)), (0.42, (0, -0.45, 0)), (0.55, (0, 0, 0))]})
    c["Hit"] = (0.35, {CHEST: [(0, I), (0.08, (22, 12, 0)), (0.35, I)], HEAD: [(0, I), (0.08, (18, 0, 0)), (0.35, I)],
                       ARM_R: [(0, (25, 0, 8)), (0.08, (-20, 0, 35)), (0.35, (25, 0, 8))], ARM_L: [(0, (10, 0, -8)), (0.08, (-20, 0, -35)), (0.35, (10, 0, -8))],
                       "move": [(0, (0, 0, 0)), (0.08, (0, -0.04, 0.08)), (0.35, (0, 0, 0))]})
    c["Die"] = (1.1, {HIPS: [(0, I), (0.25, (25, 0, 0)), (0.7, (88, 0, 6)), (1.1, (90, 0, 6))],
                      CHEST: [(0, I), (0.25, (15, 0, 0)), (0.7, (-6, 0, 0))], HEAD: [(0, I), (0.7, (-12, 20, 0))],
                      ARM_R: [(0, (25, 0, 8)), (0.7, (-30, 0, 70))], ARM_L: [(0, (10, 0, -8)), (0.7, (-40, 0, -60))],
                      LEG_L: [(0, I), (0.3, (35, 0, 0)), (0.7, (10, 0, -8))], LEG_R: [(0, I), (0.7, (-5, 0, 10))],
                      SHIN_L: [(0, I), (0.3, (-50, 0, 0)), (0.7, (-12, 0, 0))],
                      "move": [(0, (0, 0, 0)), (0.25, (0, -0.1, 0.1)), (0.7, (0, -0.8, 0.5)), (1.1, (0, -0.82, 0.5))]})
    if style == "archer":
        aim = {ARM_L: [(0, (10, 0, -8)), (0.35, (88, 0, -6)), (0.9, (88, 0, -6)), (1.2, (10, 0, -8))], FORE_L: [(0, (30, 0, 0)), (0.35, (4, 0, 0)), (0.9, (4, 0, 0)), (1.2, (30, 0, 0))],
               ARM_R: [(0, (25, 0, 8)), (0.35, (85, -25, 10)), (0.8, (85, 10, 30)), (0.9, (80, 20, 40)), (1.2, (25, 0, 8))],
               FORE_R: [(0, (35, 0, 0)), (0.35, (60, 0, 0)), (0.8, (120, 0, 0)), (0.9, (40, 0, 0)), (1.2, (35, 0, 0))],
               CHEST: [(0, I), (0.35, (0, 12, 0)), (0.9, (0, 12, 0)), (1.2, I)]}
        c["Shoot"] = (1.2, aim)
    if style == "boss":
        c["Slam"] = swing(1.5, (200, 0, 20), (-20, 0, 5), (20, 0, 0), (-38, 0, 0))
        c["Sweep"] = swing(1.3, (110, -95, 40), (25, 110, 0), (0, -60, 0), (-10, 75, 0))
        c["Roar"] = (1.6, {CHEST: [(0, I), (0.4, (20, 0, 0)), (1.2, (20, 0, 0)), (1.6, I)], HEAD: [(0, I), (0.4, (25, 0, 0)), (1.2, (25, 0, 0)), (1.6, I)],
                           ARM_R: [(0, (25, 0, 8)), (0.4, (-20, 0, 75)), (1.2, (-20, 0, 75)), (1.6, (25, 0, 8))],
                           ARM_L: [(0, (10, 0, -8)), (0.4, (-20, 0, -75)), (1.2, (-20, 0, -75)), (1.6, (10, 0, -8))]})
        c["Kneel"] = (1.2, {HIPS: [(0, I), (0.6, (-12, 0, 0))], CHEST: [(0, I), (0.6, (-30, 0, 0))], HEAD: [(0, I), (0.6, (-35, 0, 0))],
                            LEG_L: [(0, I), (0.6, (80, 0, 0))], SHIN_L: [(0, I), (0.6, (-95, 0, 0))],
                            LEG_R: [(0, I), (0.6, (-5, 0, 0))], SHIN_R: [(0, I), (0.6, (-85, 0, 0))],
                            ARM_R: [(0, (25, 0, 8)), (0.6, (55, 0, 12))], FORE_R: [(0, (35, 0, 0)), (0.6, (10, 0, 0))],
                            "move": [(0, (0, 0, 0)), (0.6, (0, -0.42, 0))]})
    return c

METAL, MAILT, CLOTH, LEATHER, BONE, WOODT = "tex_metal.jpg", "tex_mail.jpg", "tex_cloth.jpg", "tex_leather.jpg", "tex_bone.jpg", "tex_wood.jpg"
PALETTES = {
    "knight": {"plate": ((0.40, 0.41, 0.45), METAL), "mail": ((0.24, 0.24, 0.27), MAILT), "trim": ((0.52, 0.43, 0.27), METAL),
               "cloth": ((0.2, 0.25, 0.27), CLOTH), "leather": ((0.28, 0.2, 0.15), LEATHER), "blade": ((0.74, 0.76, 0.8), METAL),
               "dark": ((0.2, 0.2, 0.23), None), "black": ((0.02, 0.02, 0.03), None)},
    "boss": {"plate": ((0.26, 0.24, 0.3), METAL), "mail": ((0.16, 0.15, 0.18), MAILT), "trim": ((0.85, 0.36, 0.14), METAL),
             "cloth": ((0.45, 0.05, 0.05), CLOTH), "leather": ((0.16, 0.11, 0.09), LEATHER), "blade": ((0.62, 0.26, 0.2), METAL),
             "bone": ((0.78, 0.72, 0.6), BONE), "dark": ((0.06, 0.06, 0.08), None), "black": ((1.0, 0.42, 0.1), None, (3.0, 1.1, 0.25))},
    "bone": {"bone": ((0.72, 0.68, 0.57), BONE), "rusty": ((0.4, 0.28, 0.21), METAL), "rag": ((0.2, 0.17, 0.16), CLOTH),
             "wood": ((0.75, 0.6, 0.45), WOODT), "leather": ((0.4, 0.27, 0.17), LEATHER), "dark": ((0.2, 0.18, 0.17), None),
             "black": ((0.02, 0.015, 0.015), None)},
}

# ------------------------------------------------------------------ textures
def noise(size, cells, seed):
    """Tiling value noise in 0..1."""
    rng = np.random.default_rng(seed)
    g = rng.random((cells, cells))
    t = np.arange(size) * cells / size
    i, f = t.astype(int), t - t.astype(int)
    f = f * f * (3 - 2 * f)
    a, b = g[np.ix_(i % cells, i % cells)], g[np.ix_(i % cells, (i + 1) % cells)]
    c, d = g[np.ix_((i + 1) % cells, i % cells)], g[np.ix_((i + 1) % cells, (i + 1) % cells)]
    fx, fy = f[None, :], f[:, None]
    return (a * (1 - fx) + b * fx) * (1 - fy) + (c * (1 - fx) + d * fx) * fy

def fbm(size, seed, octaves=(4, 8, 16, 32, 64)):
    out = sum(noise(size, c, seed + k) / (2 ** k) for k, c in enumerate(octaves))
    return (out - out.min()) / (out.max() - out.min())

LAST_RELIEF = []

def relief(height, strength):
    """Shades a height map as if lit from the upper left (a little relief painted into the colour).
    The height is also remembered, so the next save_texture writes a matching normal map."""
    LAST_RELIEF[:] = [(height, strength)]
    return 1 + strength * ((height - np.roll(height, 2, 1)) + (height - np.roll(height, 2, 0)))

def normal_map(height, strength):
    """Tangent-space normal map (OpenGL convention: green points along +v) from a height map."""
    k = strength * height.shape[0] / 256.0
    du = (np.roll(height, -2, 1) - np.roll(height, 2, 1)) * k
    dv = (np.roll(height, -2, 0) - np.roll(height, 2, 0)) * k
    n = np.stack([-du, -dv, np.ones_like(height)], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    return n * 0.5 + 0.5

def save_texture(name, rgb):
    if LAST_RELIEF:
        height, strength = LAST_RELIEF.pop()
        nm = Image.fromarray((np.clip(normal_map(height, strength), 0, 1) * 255).astype(np.uint8), "RGB")
        nm.save(os.path.join(OUT, name.replace(".jpg", "_n.jpg").replace(".png", "_n.png")), quality=92, subsampling=0)
    img = Image.fromarray((np.clip(rgb, 0, 1) * 255).astype(np.uint8), "RGB")
    if name.endswith(".jpg"):
        img.save(os.path.join(OUT, name), quality=90, optimize=True, subsampling=0)
    else:
        img.save(os.path.join(OUT, name), optimize=True)

def bricks(size, rows, cols, seed, stagger=True, joint=0.07):
    """Returns (mortar 0..1, a random value per brick, rounded height of each brick)."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:size, 0:size] / size
    row = np.floor(y * rows).astype(int)
    xs = x * cols + (row % 2) * 0.5 * stagger
    col = np.floor(xs).astype(int) % cols
    fy, fx = y * rows - row, xs - np.floor(xs)
    edge = np.minimum(np.minimum(fx, 1 - fx) * (rows / cols) * 2.2, np.minimum(fy, 1 - fy))
    return np.clip(1 - edge / joint, 0, 1), rng.random((rows, cols))[row, col], np.clip(edge / 0.22, 0, 1) ** 0.6

def grey(v, tint=(1, 1, 1)):
    return np.stack([v * tint[0], v * tint[1], v * tint[2]], -1)

def make_textures():
    S = 1024   # high resolution: walls and floors are seen up close
    grain, fine, pits = fbm(S, 1), fbm(S, 7, (32, 64, 128)), fbm(S, 5, (64, 128, 256))
    # castle wall: weathered blocks, chipped edges, damp streaks and moss
    mortar, tone, dome = bricks(S, 8, 4, 2)
    height = dome * 0.8 + grain * 0.5 + pits * 0.25
    streak = fbm(S, 40, (2, 4, 8))[:, (np.arange(S) // 6) % S] * 0.12
    v = (0.5 + (tone - 0.5) * 0.22 + (grain - 0.5) * 0.2 - streak) * relief(height, 1.6) * (1 - mortar * 0.55)
    moss = np.clip((fbm(S, 11) - 0.6) * 4, 0, 1)[..., None] * (0.4 + 0.6 * mortar[..., None])
    save_texture("tex_stone.jpg", grey(v, (1.0, 0.98, 0.95)) * (1 - moss * 0.55) + np.array([0.2, 0.28, 0.12]) * moss * 0.55)
    # cobbles: rounded stones, each with its own colour, dirt between them
    mortar, tone, dome = bricks(S, 12, 12, 3, joint=0.16)
    v = (0.42 + (tone - 0.5) * 0.3 + (fine - 0.5) * 0.12) * relief(dome + pits * 0.2, 2.2)
    warm = (tone[..., None] - 0.5) * np.array([0.06, 0.0, -0.06])
    save_texture("tex_cobble.jpg", (grey(v, (1.02, 0.98, 0.93)) + warm) * (1 - mortar[..., None] * 0.72))
    # hall floor: worn flagstones with cracks
    mortar, tone, dome = bricks(S, 4, 4, 5, stagger=False, joint=0.04)
    cracks = np.clip(1 - np.abs(fbm(S, 61, (3, 6, 12, 24)) - 0.5) * 60, 0, 1)
    v = (0.3 + (tone - 0.5) * 0.1 + (grain - 0.5) * 0.12) * relief(dome * 0.5 + grain * 0.6 - cracks * 0.4, 1.5)
    save_texture("tex_flag.jpg", grey(v, (0.95, 0.95, 1.05)) * (1 - mortar[..., None] * 0.6) * (1 - cracks[..., None] * 0.5))
    # grass: fine blades over uneven ground, with trodden earth here and there
    g, blades = fbm(S, 21, (3, 6, 12, 24)), fbm(S, 23, (128, 256))
    earth = np.clip((fbm(S, 29, (2, 4, 8, 16)) - 0.68) * 7, 0, 1)[..., None]
    green = np.stack([0.2 + g * 0.1 + blades * 0.08, 0.22 + g * 0.1 + blades * 0.1, 0.12 + g * 0.05], -1) * relief(blades, 0.9)[..., None]   # dry, dying grass
    save_texture("tex_grass.jpg", green * (1 - earth) + np.array([0.3, 0.24, 0.17]) * (0.75 + pits[..., None] * 0.5) * earth)
    S = 512
    grain, fine = fbm(S, 1), fbm(S, 7, (32, 64, 128))
    y, x = np.mgrid[0:S, 0:S] / S
    # planks with grain and nail heads
    plank = np.floor(x * 4)
    streak = fbm(S, 31, (2, 4, 8))[:, (np.arange(S) * 7) % S]
    rings = 0.5 + 0.5 * np.sin(y * 46 + streak * 16 + plank * 5)
    gap = np.clip(1 - np.minimum(x * 4 - plank, 1 - (x * 4 - plank)) / 0.05, 0, 1)
    v = (0.44 + rings * 0.09 + (np.random.default_rng(4).random(4)[plank.astype(int)] - 0.5) * 0.12) * relief(rings * 0.3 - gap, 1.2)
    save_texture("tex_wood.jpg", grey(v, (1.1, 0.78, 0.5)) * (1 - gap[..., None] * 0.7))
    mortar, tone, dome = bricks(S, 8, 6, 9)
    v = (0.4 + (tone - 0.5) * 0.25 + (fine - 0.5) * 0.1) * relief(dome, 1.5)
    save_texture("tex_roof.jpg", grey(v, (0.62, 0.66, 0.78)) * (1 - mortar[..., None] * 0.55))
    # plate: brushed steel with scratches and dents; tinted per material (steel, gold, rust)
    brushed = fbm(S, 51, (2, 4))[:, (np.arange(S) * 13) % S] * 0.5 + noise(S, 128, 52)[(np.arange(S) * 3) % S][:, :] * 0.5
    scratches = np.clip(1 - np.abs(fbm(S, 53, (5, 9, 17)) - 0.5) * 70, 0, 1)
    v = (0.78 + (brushed - 0.5) * 0.16 + (grain - 0.5) * 0.14 + scratches * 0.2) * relief(grain * 0.8 - scratches * 0.3, 0.8)
    rust = np.clip((fbm(S, 57, (3, 6, 12, 24)) - 0.55) * 5, 0, 1)[..., None]
    save_texture("tex_metal.jpg", grey(v) * (1 - rust * 0.75) + np.array([0.62, 0.36, 0.2]) * (0.7 + fine[..., None] * 0.5) * rust * 0.75)
    # mail: rows of interlocked rings
    rings = np.maximum(np.sin(x * math.tau * 16) * np.sin(y * math.tau * 16), np.sin(x * math.tau * 16 + math.pi / 2) * np.sin(y * math.tau * 16 + math.pi / 2))
    hole = np.clip(rings * 1.4, 0, 1)
    save_texture("tex_mail.jpg", grey((0.35 + hole * 0.6) * relief(hole, 1.8)))
    # cloth: a visible weave
    weave = (np.sin(x * math.tau * 48) * 0.5 + 0.5) * (np.sin(y * math.tau * 48) * 0.5 + 0.5)
    save_texture("tex_cloth.jpg", grey((0.74 + weave * 0.22 + (grain - 0.5) * 0.2) * relief(weave, 0.5)))
    # leather: creased
    crease = np.clip(1 - np.abs(fbm(S, 71, (6, 12, 24)) - 0.5) * 18, 0, 1)
    save_texture("tex_leather.jpg", grey((0.8 + (grain - 0.5) * 0.25 - crease * 0.3) * relief(-crease, 0.9)))
    # bone: porous, stained in the hollows, with hairline cracks
    crack = np.clip(1 - np.abs(fbm(S, 81, (4, 8, 16)) - 0.5) * 50, 0, 1)
    stain = fbm(S, 83, (3, 6))
    v = (0.9 + (fine - 0.5) * 0.14 - stain * 0.16 - crack * 0.35) * relief(fine * 0.6 - crack, 0.8)
    save_texture("tex_bone.jpg", grey(v, (1.0, 0.97, 0.88)))

# ------------------------------------------------------------------ scenery
MAT = {"stone": ((1, 1, 1), "tex_stone.jpg"), "cobble": ((1, 1, 1), "tex_cobble.jpg"), "flag": ((1, 1, 1), "tex_flag.jpg"),
       "grass": ((1, 1, 1), "tex_grass.jpg"), "wood": ((1, 1, 1), "tex_wood.jpg"), "roof": ((1, 1, 1), "tex_roof.jpg"),
       "iron": ((0.22, 0.22, 0.25), "tex_metal.jpg"), "cloth": ((0.3, 0.07, 0.07), "tex_cloth.jpg"), "gold": ((0.95, 0.74, 0.3), "tex_metal.jpg"),
       "leaf": ((0.09, 0.13, 0.11), None), "leaf2": ((0.12, 0.16, 0.12), None), "bark": ((0.5, 0.36, 0.26), "tex_wood.jpg"),
       "rock": ((0.4, 0.4, 0.43), None), "glass": ((0.75, 0.1, 0.12), None, (0.35, 0.02, 0.02), 0.6), "glow": ((0.5, 0.85, 1.0), None, (0.5, 1.4, 2.2)),
       "black": ((0.03, 0.03, 0.04), None)}

def wall(length, height=6.0, thick=1.6, crenels=True):
    m = Mesh()
    m.box("stone", (0, height / 2, 0), (length, height, thick))
    m.box("stone", (0, 0.45, 0), (length, 0.9, thick + 0.5))                       # plinth
    m.box("stone", (0, height - 0.75, 0), (length, 0.22, thick + 0.24))           # string course
    n = max(int(length // 2), 1)
    for i in range(max(int(length // 4), 1)):                                     # arrow slits and buttresses
        x = -length / 2 + (i + 0.5) * length / max(int(length // 4), 1)
        for z in (-thick / 2 - 0.012, thick / 2 + 0.012):
            m.box("black", (x, height * 0.55, z), (0.16, 1.3, 0.03))
            m.box("black", (x, height * 0.55, z), (0.5, 0.16, 0.03))
        if length >= 10 and i % 2 == 0:
            for z in (-thick / 2 - 0.3, thick / 2 + 0.3):
                m.box("stone", (x + length / 8, height * 0.36, z), (1.0, height * 0.72, 0.6))
                m.box("stone", (x + length / 8, height * 0.75, z * 0.93), (1.0, 0.4, 0.45), r=(18 if z < 0 else -18, 0, 0))
    if crenels:
        m.box("stone", (0, height + 0.15, 0), (length + 0.2, 0.3, thick + 0.4))
        for i in range(n):
            x = -length / 2 + (i + 0.5) * length / n
            for z in (-thick / 2 - 0.05, thick / 2 + 0.05):
                m.box("stone", (x, height + 0.78, z), (length / n * 0.56, 0.96, 0.4))
                m.box("stone", (x, height + 1.3, z), (length / n * 0.62, 0.12, 0.5))
    return m

def ruin():
    """What is left of a wall: broken courses of stone and the rubble at its foot."""
    m = Mesh()
    rng = np.random.default_rng(8)
    for i in range(6):
        h = [3.4, 4.6, 2.6, 1.5, 2.2, 0.9][i]
        m.box("stone", (-3.75 + i * 1.5, h / 2, 0), (1.5, h, 1.4))
        m.box("stone", (-3.75 + i * 1.5 + 0.2, h + 0.2, 0.1), (0.9, 0.45, 1.0), r=(rng.uniform(-8, 8), rng.uniform(-20, 20), rng.uniform(-10, 10)))
    for _ in range(14):
        x, z, s = rng.uniform(-4.5, 4.5), rng.uniform(0.9, 2.6) * rng.choice([-1, 1]), rng.uniform(0.3, 0.75)
        m.box("stone", (x, s * 0.35, z), (s * 1.3, s * 0.7, s), r=(rng.uniform(-15, 15), rng.uniform(0, 90), rng.uniform(-15, 15)))
    return m

def tower():
    m = Mesh()
    m.lathe("stone", spline([(0, 3.7), (1.2, 3.2), (4, 3.05), (10.6, 3.0)], 3) + [(11.0, 3.45), (11.5, 3.6), (12.0, 3.6)], sides=24)
    for k in range(12):                                                           # corbels under the parapet
        a = k / 12 * math.tau
        m.box("stone", (math.cos(a) * 3.25, 10.75, math.sin(a) * 3.25), (0.5, 0.5, 0.45), r=(0, -math.degrees(a), 0))
    for k in range(12):
        a = (k + 0.5) / 12 * math.tau
        if k % 2 == 0:
            m.box("stone", (math.cos(a) * 3.38, 12.55, math.sin(a) * 3.38), (0.5, 1.1, 1.35), r=(0, -math.degrees(a), 0))
    for k in range(4):                                                            # slits, two floors
        a = (k + 0.5) / 4 * math.tau
        for y in (4.5, 8.2):
            m.box("black", (math.cos(a) * 3.03, y, math.sin(a) * 3.03), (0.08, 1.3, 0.2), r=(0, -math.degrees(a), 0))
    m.lathe("roof", [(11.9, 3.3), (12.4, 2.9), (16.6, 0.0)], sides=24)
    m.lathe("gold", [(16.4, 0.06), (17.7, 0.02)], sides=6)
    m.tube("cloth", [(0.05, 17.4, 0), (0.5, 17.45, 0.06), (0.95, 17.35, -0.04)], [0.26, 0.24, 0.2], sides=4, flatten=0.08)
    return m

def pillar():
    m = Mesh()
    m.box("stone", (0, 0.25, 0), (1.6, 0.5, 1.6))
    m.lathe("stone", spline([(0.5, 0.68), (0.62, 0.6), (0.8, 0.5), (3.4, 0.48), (6.1, 0.44), (6.3, 0.55), (6.5, 0.68)], 3), sides=20)
    for k in range(10):                                                           # fluting
        a = k / 10 * math.tau
        m.tube("stone", [(math.cos(a) * 0.5, 0.9, math.sin(a) * 0.5), (math.cos(a) * 0.46, 6.0, math.sin(a) * 0.46)], 0.05, sides=5)
    m.box("stone", (0, 6.75, 0), (1.7, 0.5, 1.7))
    return m

def portcullis():
    m = Mesh()
    for i in range(7):
        m.tube("iron", [(-2.4 + i * 0.8, 0.0, 0), (-2.4 + i * 0.8, 5.2, 0)], 0.07, sides=4)
        m.lathe("iron", [(-0.3, 0.0), (0.0, 0.11)], center=(-2.4 + i * 0.8, 0), sides=4)
    for y in (0.8, 2.0, 3.2, 4.4):
        m.box("iron", (0, y, 0), (5.4, 0.12, 0.1))
        for i in range(7):
            m.lathe("iron", ball(y, 0.09), center=(-2.4 + i * 0.8, -0.08), sides=6)    # rivets
    m.box("wood", (0, 5.3, 0), (5.6, 0.35, 0.3))
    return m

def torch():
    m = Mesh()
    m.lathe("iron", spline([(0, 0.24), (0.08, 0.2), (0.14, 0.05), (1.0, 0.04), (1.9, 0.045), (2.0, 0.17), (2.25, 0.25)], 3), sides=12)
    for k in range(3):
        a = k / 3 * math.tau
        m.tube("iron", [(0, 0.5, 0), (math.cos(a) * 0.3, 0.0, math.sin(a) * 0.3)], 0.02, sides=4)
    return m

def barrel():
    m = Mesh()
    m.lathe("wood", spline([(0, 0.36), (0.25, 0.43), (0.5, 0.45), (0.75, 0.43), (1.0, 0.36)], 3), sides=20)
    for y in (0.14, 0.4, 0.6, 0.86):
        r = 0.458 - abs(y - 0.5) * 0.16
        m.lathe("iron", [(y - 0.03, r), (y + 0.03, r)], sides=20)
    return m

def crate():
    m = Mesh()
    m.box("wood", (0, 0.5, 0), (0.96, 0.96, 0.96))
    for a, b in ((-0.47, -0.47), (-0.47, 0.47), (0.47, -0.47), (0.47, 0.47)):
        m.box("bark", (a, 0.5, b), (0.1, 1.02, 0.1))
        m.box("bark", (a, 0.02 if b < 0 else 0.98, 0), (0.1, 0.1, 1.0))
        m.box("bark", (0, 0.02 if a < 0 else 0.98, b), (1.0, 0.1, 0.1))
    for z in (-0.49, 0.49):
        m.box("bark", (0, 0.5, z), (0.09, 1.3, 0.03), r=(0, 0, 45))
    return m

def pine():
    m = Mesh()
    rng = np.random.default_rng(3)
    m.lathe("bark", spline([(0, 0.34), (0.3, 0.24), (2.0, 0.2), (5.5, 0.05)], 2), sides=10)
    y, r = 1.3, 2.1
    for layer in range(7):                                                        # drooping boughs, layer on layer
        h = 1.5 - layer * 0.08
        prof = [(y - 0.35, r), (y - 0.1, r * 0.82), (y + h * 0.5, r * 0.36), (y + h, 0.02)]
        m.lathe("leaf" if layer % 2 else "leaf2", prof, sides=14, flat=True, r=(rng.uniform(-4, 4), rng.uniform(0, 360), rng.uniform(-4, 4)), pivot=(0, y, 0))
        y += h * 0.52
        r *= 0.8
    for vs in (m.tris["leaf"], m.tris["leaf2"]):                                  # ragged edges
        for v in vs:
            v[0][:] = v[0] + np.sin(v[0] * 5.3 + 1.7) * 0.07
    return m

def rock():
    m = Mesh()
    m.lathe("rock", [(0, 0.95), (0.35, 1.2), (0.85, 1.05), (1.25, 0.6), (1.45, 0.0)], sides=9, sx=1.25, sz=0.95, flat=True)
    return m

def banner():
    m = Mesh()
    m.lathe("wood", [(0, 0.07), (5.0, 0.05)], sides=8)
    m.lathe("gold", [(5.0, 0.05), (5.1, 0.1), (5.4, 0.0)], sides=6)
    m.tube("wood", [(-0.1, 4.8, 0), (1.4, 4.8, 0)], 0.04, sides=6)
    for i in range(6):                                                            # cloth hanging in folds
        x = 0.15 + i * 0.2
        long = 2.3 - (0.5 if i in (1, 4) else 0.0) - (0.25 if i in (0, 5) else 0)
        m.tube("cloth", [(x, 4.78, 0), (x, 4.0, 0.05 * math.sin(i * 1.9)), (x, 4.78 - long, 0.08 * math.sin(i * 1.3 + 1))], [0.105, 0.105, 0.1], sides=4, flatten=0.12)
    m.box("gold", (0.65, 3.9, 0.035), (0.34, 0.5, 0.012))
    return m

def shrine():
    m = Mesh()
    m.lathe("stone", spline([(0, 1.15), (0.25, 1.05), (0.32, 0.7), (0.7, 0.55), (1.0, 0.55), (1.1, 0.82), (1.25, 0.85)], 2), sides=8, flat=True)
    m.lathe("glow", [(1.35, 0.0), (1.75, 0.28), (2.4, 0.0)], sides=6, flat=True)
    for k in range(4):
        a = (k + 0.5) / 4 * math.tau
        m.lathe("glow", [(1.3, 0.0), (1.5, 0.09), (1.8, 0.0)], center=(math.cos(a) * 0.5, math.sin(a) * 0.5), sides=5, flat=True)
    return m

def potion():
    m = Mesh()
    m.lathe("glass", spline([(0, 0.0), (0.02, 0.14), (0.12, 0.17), (0.22, 0.14), (0.3, 0.06), (0.4, 0.05)], 3), sides=12)
    m.lathe("wood", [(0.4, 0.06), (0.47, 0.05)], sides=8)
    return m

def slab(mat, w, d):
    m = Mesh()
    m.box(mat, (0, -0.5, 0), (w, 1.0, d))
    return m

def throne():
    m = Mesh()
    m.box("stone", (0, 0.2, 0), (3.6, 0.4, 3.0))
    m.box("stone", (0, 0.5, 0.1), (3.0, 0.2, 2.4))
    m.box("stone", (0, 0.95, 0.1), (1.5, 0.7, 1.3))
    m.box("stone", (0, 2.6, 0.75), (1.7, 4.0, 0.35))
    m.lathe("stone", [(4.6, 0.85), (5.3, 0.0)], center=(0, 0.75), sides=4, sz=0.25, flat=True)
    for x in (-0.85, 0.85):
        m.box("stone", (x, 1.5, 0.1), (0.25, 0.5, 1.3))
        m.lathe("gold", ball(1.85, 0.13), center=(x, -0.5), sides=10)
        m.lathe("gold", [(4.5, 0.12), (5.2, 0.0)], center=(x * 0.9, 0.75), sides=6)
    m.box("cloth", (0, 2.6, 0.56), (1.1, 3.4, 0.03))
    m.box("cloth", (0, 1.31, 0.05), (1.2, 0.04, 1.1))
    return m

if __name__ == "__main__":
    for old in os.listdir(OUT):
        os.remove(os.path.join(OUT, old))
    make_textures()
    for name, mesh in (("wall_12", wall(12)), ("wall_6", wall(6)), ("wall_hall", wall(12, 8.0, 1.2, False)),
                       ("pier", wall(3.5, 8.0, 3.2)), ("lintel", wall(6.0, 2.6, 3.2)), ("tower", tower()), ("pillar", pillar()),
                       ("portcullis", portcullis()), ("torch", torch()), ("barrel", barrel()), ("crate", crate()),
                       ("pine", pine()), ("rock", rock()), ("ruin", ruin()), ("banner", banner()), ("shrine", shrine()), ("potion", potion()),
                       ("throne", throne()), ("ground_grass", slab("grass", 80, 56)), ("ground_cobble", slab("cobble", 36, 36)),
                       ("ground_hall", slab("flag", 26, 34))):
        write_obj(name, mesh, MAT, tile=4.0 if name.startswith("ground") else 3.0)
    for style, palette in (("knight", "knight"), ("boss", "boss"), ("skeleton", "bone"), ("archer", "bone"), ("brute", "bone")):
        print(f"{style}: {write_gltf(style, humanoid(style), PALETTES[palette], JOINTS, clips_for(style))} triangles")
    # the list of files the editor embeds
    files = sorted(f for f in os.listdir(OUT))
    rs = "// Generated by tools/make_ashen_assets.py. Do not edit.\npub const FILES: &[(&str, &[u8])] = &[\n"
    rs += "".join(f'    ("{f}", include_bytes!("../template/ashen/{f}")),\n' for f in files) + "];\n"
    open(os.path.join(OUT, "..", "..", "src", "ashen_files.rs"), "w").write(rs)
    total = sum(os.path.getsize(os.path.join(OUT, f)) for f in os.listdir(OUT))
    print(f"{len(os.listdir(OUT))} files, {total / 1024:.0f} KB in {os.path.normpath(OUT)}")
