#!/usr/bin/env python3
"""Builds the models and textures of the "Outpost Kestrel" first-person example from code.

    python3 tools/make_outpost_assets.py

Uses the mesh builder of make_ashen_assets.py. Output: crates/cryo-editor/template/outpost/.
"""
import math, os
import numpy as np
import make_ashen_assets as A
from make_ashen_assets import Mesh, spline, ball, fbm, noise, relief, grey, bricks

OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "cryo-editor", "template", "outpost")
os.makedirs(OUT, exist_ok=True)
A.OUT = OUT

def textures():
    S = 512
    grain, fine, pits = fbm(S, 101), fbm(S, 107, (32, 64, 128)), fbm(S, 105, (64, 128, 256))
    y, x = np.mgrid[0:S, 0:S] / S
    # poured concrete: form lines, pores, stains, hairline cracks
    seam = np.clip(1 - np.minimum(np.abs((y * 2) % 1 - 0.5), np.abs((x * 2) % 1 - 0.5)) / 0.012, 0, 1)
    crack = np.clip(1 - np.abs(fbm(S, 111, (3, 6, 12, 24)) - 0.5) * 55, 0, 1)
    stain = fbm(S, 113, (2, 4, 8))
    holes = np.clip((pits - 0.78) * 9, 0, 1)
    v = (0.5 + (grain - 0.5) * 0.16 + (fine - 0.5) * 0.08 - stain * 0.12 - holes * 0.25 - crack * 0.3 - seam * 0.25)
    A.save_texture("tex_concrete.png", grey(v * relief(fine * 0.5 - holes - crack - seam, 1.0), (1.0, 0.99, 0.96)))
    # floor: concrete slabs with worn yellow guide lines
    v2 = 0.36 + (grain - 0.5) * 0.14 + (fine - 0.5) * 0.1 - holes * 0.2 - seam * 0.3
    line = (np.abs(x - 0.5) < 0.035) * np.clip(fbm(S, 117, (16, 32, 64)) * 1.6 - 0.3, 0, 1)
    A.save_texture("tex_floor.png", grey(v2 * relief(fine * 0.5 - holes - seam, 1.0)) * (1 - line[..., None]) + np.array([0.75, 0.6, 0.12]) * line[..., None] * (0.7 + fine[..., None] * 0.4))
    # wall panels: riveted steel plates with grime running down
    mortar, tone, dome = bricks(S, 2, 2, 121, stagger=False, joint=0.03)
    rivet = np.zeros((S, S))
    for cy in np.arange(0.03, 1, 0.117):
        for cx in (0.03, 0.47, 0.53, 0.97):
            rivet = np.maximum(rivet, np.clip(1 - np.hypot(x - cx, y - cy) / 0.012, 0, 1))
            rivet = np.maximum(rivet, np.clip(1 - np.hypot(x - cy, y - cx) / 0.012, 0, 1))
    grime = fbm(S, 123, (2, 4, 8))[:, (np.arange(S) // 5) % S]
    brushed = fbm(S, 125, (2, 4))[(np.arange(S) * 11) % S, :]
    v3 = 0.5 + (tone - 0.5) * 0.08 + (brushed - 0.5) * 0.12 - grime * 0.2 + rivet * 0.25
    A.save_texture("tex_panel.png", grey(v3 * relief(rivet * 1.5 - mortar * 1.2 + fine * 0.2, 1.1) * (1 - mortar * 0.6), (0.92, 0.97, 1.05)))
    # hazard stripes, chipped
    stripe = (((x + y) * 8) % 1 < 0.5)
    chip = np.clip((fbm(S, 127, (8, 16, 32, 64)) - 0.62) * 8, 0, 1)
    col = np.where(stripe[..., None], np.array([0.85, 0.65, 0.08]), np.array([0.07, 0.07, 0.08])) * (0.85 + fine[..., None] * 0.3)
    A.save_texture("tex_hazard.png", col * (1 - chip[..., None]) + grey(0.4 + fine * 0.2) * chip[..., None])
    # shipping-container ribs
    ribs = 0.5 + 0.5 * np.sin(x * math.tau * 14)
    rust = np.clip((fbm(S, 131, (3, 6, 12, 24)) - 0.56) * 5, 0, 1)[..., None]
    v5 = (0.62 + (grain - 0.5) * 0.12) * relief(ribs * 1.6, 1.3)
    A.save_texture("tex_container.png", grey(v5) * (1 - rust * 0.7) + np.array([0.5, 0.28, 0.16]) * (0.7 + fine[..., None] * 0.5) * rust * 0.7)
    # dark gunmetal with wear on the edges of a grid of plates
    mortar, tone, dome = bricks(S, 6, 6, 133, joint=0.05)
    v6 = (0.55 + (tone - 0.5) * 0.1 + (fine - 0.5) * 0.14 + mortar * 0.18) * relief(dome * 0.6 + fine * 0.3, 1.0)
    A.save_texture("tex_gunmetal.png", grey(v6))
    # floor grating
    g = np.maximum(np.clip(1 - np.abs((x * 16) % 1 - 0.5) / 0.18, 0, 1), np.clip(1 - np.abs((y * 16) % 1 - 0.5) / 0.18, 0, 1))
    A.save_texture("tex_grate.png", grey((0.12 + g * 0.5 + (fine - 0.5) * 0.1) * relief(g, 1.6)))

M = {"concrete": ((1, 1, 1), "tex_concrete.png"), "floor": ((1, 1, 1), "tex_floor.png"), "panel": ((1, 1, 1), "tex_panel.png"),
     "hazard": ((1, 1, 1), "tex_hazard.png"), "grate": ((1, 1, 1), "tex_grate.png"), "gunmetal": ((0.42, 0.44, 0.48), "tex_gunmetal.png"),
     "blue": ((0.25, 0.42, 0.62), "tex_container.png"), "red": ((0.62, 0.22, 0.18), "tex_container.png"), "green": ((0.26, 0.42, 0.3), "tex_container.png"),
     "steel": ((0.7, 0.72, 0.76), "tex_gunmetal.png"), "black": ((0.03, 0.03, 0.035), None), "rubber": ((0.08, 0.08, 0.09), None),
     "lamp": ((1.0, 0.9, 0.7), None), "eye": ((1.0, 0.25, 0.1), None), "cyan": ((0.3, 0.9, 1.0), None), "white": ((0.9, 0.9, 0.9), None),
     "gun_dark": ((0.05, 0.05, 0.06), None), "gun_mid": ((0.13, 0.135, 0.15), None), "gun_tan": ((0.3, 0.26, 0.19), None),
     "medic": ((0.85, 0.15, 0.15), None), "ammo": ((0.35, 0.4, 0.25), "tex_gunmetal.png"), "brass": ((0.85, 0.65, 0.3), None)}

def wall(length, height=7.0, thick=0.8, mat="concrete"):
    m = Mesh()
    m.box(mat, (0, height / 2, 0), (length, height, thick))
    m.box("hazard", (0, 0.2, 0), (length, 0.4, thick + 0.08))          # kick plate
    for i in range(int(length // 4) + 1):                             # steel ribs and a pipe run
        x = -length / 2 + i * 4.0
        if abs(x) <= length / 2:
            for z in (-thick / 2 - 0.08, thick / 2 + 0.08):
                m.box("gunmetal", (x, height / 2, z), (0.3, height, 0.16))
    for z in (-thick / 2 - 0.2, thick / 2 + 0.2):
        m.tube("steel", [(-length / 2, height - 1.2, z), (length / 2, height - 1.2, z)], 0.09, sides=8)
        m.tube("gunmetal", [(-length / 2, height - 1.5, z), (length / 2, height - 1.5, z)], 0.05, sides=6)
    return m

def slab(mat, w, d, y=-0.5):
    m = Mesh()
    m.box(mat, (0, y, 0), (w, 1.0, d))
    return m

def container(mat):
    m = Mesh()
    m.box(mat, (0, 1.3, 0), (6.0, 2.6, 2.44))
    for x in (-2.95, 2.95):
        for z in (-1.17, 1.17):
            m.box("gunmetal", (x, 1.3, z), (0.16, 2.66, 0.16))
    for y in (0.04, 2.58):
        m.box("gunmetal", (0, y, 0), (6.06, 0.1, 2.5))
    for z in (-0.55, 0.55):                                           # door bars
        m.tube("steel", [(3.03, 0.2, z), (3.03, 2.4, z)], 0.03, sides=6)
    return m

def crate():
    m = Mesh()
    m.box("ammo", (0, 0.55, 0), (1.3, 1.1, 0.9))
    m.box("gunmetal", (0, 1.08, 0), (1.36, 0.08, 0.96))
    for x in (-0.45, 0.45):
        m.box("black", (x, 0.55, 0.46), (0.08, 0.5, 0.03))
    return m

def barrel():
    m = Mesh()
    m.lathe("red", spline([(0, 0.4), (0.03, 0.43), (0.6, 0.43), (1.17, 0.43), (1.2, 0.4)], 2), sides=20)
    for y in (0.38, 0.8):
        m.lathe("gunmetal", [(y - 0.02, 0.445), (y + 0.02, 0.445)], sides=20)
    m.lathe("hazard", [(0.5, 0.435), (0.7, 0.435)], sides=20)
    return m

def door():
    m = Mesh()
    m.box("panel", (0, 2.0, 0), (4.4, 4.0, 0.35))
    m.box("hazard", (0, 0.45, 0), (4.44, 0.5, 0.4))
    m.box("gunmetal", (0, 3.85, 0), (4.5, 0.3, 0.5))
    for z in (-0.2, 0.2):
        m.box("black", (0, 2.3, z), (1.2, 0.5, 0.03))
        m.lathe("eye", ball(3.3, 0.09), center=(0, z), sides=8)
    return m

def frame():   # the wall around a doorway: two posts and a header, 12 m wide in all
    m = Mesh()
    for x in (-4.1, 4.1):
        m.box("concrete", (x, 3.5, 0), (3.8, 7.0, 0.8))
    m.box("concrete", (0, 5.5, 0), (4.4, 3.0, 0.8))
    m.box("hazard", (0, 4.1, 0), (4.4, 0.25, 0.9))
    return m

def lamp():
    m = Mesh()
    m.lathe("gunmetal", spline([(0, 0.25), (0.1, 0.12), (0.3, 0.07), (5.0, 0.06)], 2), sides=10)
    m.tube("gunmetal", [(0, 5.0, 0), (0, 5.4, -0.3), (0, 5.5, -1.0)], 0.05, sides=8)
    m.box("gunmetal", (0, 5.46, -1.1), (0.35, 0.1, 0.6))
    m.box("lamp", (0, 5.4, -1.1), (0.28, 0.04, 0.5))
    return m

def drone(heavy):
    """A security drone: armoured shell, one sensor eye, ducted fans, a gun under the chin."""
    m = Mesh()
    s = 1.35 if heavy else 1.0
    shell = "red" if heavy else "gunmetal"
    m.lathe(shell, ball(0, 0.36 * s, n=10, squash=0.72), sides=20, sz=1.25)
    m.lathe("steel", [(-0.02, 0.37 * s), (0.02, 0.372 * s)], sides=20, sz=1.25)                    # seam
    m.lathe("black", ball(0.0, 0.15 * s, n=6), center=(0, -0.37 * s), sides=12)                     # eye housing
    m.lathe("eye", ball(0.0, 0.085 * s, n=6), center=(0, -0.47 * s), sides=10)
    m.tube("steel", [(0, -0.2 * s, -0.2 * s), (0, -0.24 * s, -0.75 * s)], 0.035 * s, sides=8)       # gun
    m.box("gunmetal", (0, -0.22 * s, -0.3 * s), (0.14 * s, 0.14 * s, 0.34 * s))
    for sx in (-1, 1):
        for sz in ((-1, 1) if heavy else (0,)):
            cx, cz = sx * 0.62 * s, sz * 0.36 * s
            m.tube("gunmetal", [(sx * 0.3 * s, 0, cz * 0.6), (cx, 0.02, cz)], 0.04 * s, sides=6)    # arm
            m.lathe("gunmetal", [(-0.07 * s, 0.2 * s), (0.07 * s, 0.22 * s)], center=(cx, cz), sides=16)   # duct
            m.lathe("black", [(0.0, 0.0), (0.012, 0.19 * s)], center=(cx, cz), sides=12)             # fan disc
    m.tube("steel", [(0, 0.25 * s, 0.1 * s), (0, 0.5 * s, 0.2 * s)], 0.012, sides=4)                # antenna
    return m

def rifle():
    """The player's carbine, modelled pointing along -Z with the grip at the origin."""
    m = Mesh()
    m.box("gun_mid", (0, 0.06, -0.12), (0.055, 0.1, 0.5))                 # receiver
    m.box("gun_mid", (0, 0.125, -0.1), (0.04, 0.03, 0.3))                 # top rail
    m.tube("gun_dark", [(0, 0.07, -0.36), (0, 0.07, -0.78)], 0.016, sides=10) # barrel
    m.tube("gun_mid", [(0, 0.07, -0.36), (0, 0.07, -0.6)], 0.034, sides=10)   # handguard
    m.tube("gun_dark", [(0, 0.07, -0.74), (0, 0.07, -0.82)], 0.024, sides=8)  # muzzle brake
    m.box("gun_dark", (0, -0.06, 0.02), (0.045, 0.14, 0.06), r=(18, 0, 0))    # grip
    m.box("gun_tan", (0, -0.1, -0.16), (0.04, 0.2, 0.075), r=(-10, 0, 0))     # magazine
    m.box("gun_mid", (0, 0.05, 0.22), (0.045, 0.09, 0.22))                # stock
    m.box("gun_dark", (0, 0.04, 0.34), (0.05, 0.13, 0.03))
    m.box("gun_mid", (0, 0.165, -0.06), (0.035, 0.05, 0.1))               # sight body
    m.box("cyan", (0, 0.17, -0.005), (0.022, 0.03, 0.004))                 # sight glass
    return m

def pickup(kind):
    m = Mesh()
    if kind == "medkit":
        m.box("white", (0, 0.18, 0), (0.5, 0.36, 0.34))
        m.box("medic", (0, 0.18, 0.172), (0.22, 0.07, 0.01))
        m.box("medic", (0, 0.18, 0.172), (0.07, 0.22, 0.01))
        m.tube("gunmetal", [(-0.1, 0.36, 0), (-0.1, 0.42, 0), (0.1, 0.42, 0), (0.1, 0.36, 0)], 0.012, sides=5)
    else:
        m.box("ammo", (0, 0.14, 0), (0.44, 0.28, 0.24))
        for i in range(5):
            m.lathe("brass", [(0.28, 0.02), (0.36, 0.02), (0.4, 0.0)], center=(-0.14 + i * 0.07, 0), sides=6)
    return m

def core():
    """The reactor: a ribbed column with a glowing heart."""
    m = Mesh()
    m.lathe("gunmetal", spline([(0, 2.2), (0.4, 2.0), (0.8, 1.2), (1.2, 1.1)], 2), sides=24)
    m.lathe("cyan", [(1.2, 0.8), (4.6, 0.8)], sides=20)
    for k in range(8):
        a = k / 8 * math.tau
        m.tube("gunmetal", [(math.cos(a) * 1.0, 1.2, math.sin(a) * 1.0), (math.cos(a) * 1.0, 4.6, math.sin(a) * 1.0)], 0.09, sides=6)
    for y in (2.0, 2.9, 3.8):
        m.lathe("steel", [(y - 0.06, 1.06), (y + 0.06, 1.06)], sides=24)
    m.lathe("gunmetal", spline([(4.6, 1.1), (5.0, 1.2), (5.6, 2.0), (6.0, 2.2)], 2), sides=24)
    return m

def node():
    """A coolant node: what the player has to destroy."""
    m = Mesh()
    m.lathe("gunmetal", [(0, 0.6), (0.2, 0.55), (0.3, 0.3)], sides=12)
    m.lathe("cyan", ball(0.95, 0.5, n=8, squash=1.2), sides=14)
    for k in range(4):
        a = (k + 0.5) / 4 * math.tau
        m.tube("steel", [(math.cos(a) * 0.5, 0.2, math.sin(a) * 0.5), (math.cos(a) * 0.56, 0.95, math.sin(a) * 0.56), (math.cos(a) * 0.3, 1.6, math.sin(a) * 0.3)], 0.04, sides=6)
    m.lathe("hazard", [(0.02, 0.62), (0.16, 0.6)], sides=12)
    return m

if __name__ == "__main__":
    for old in os.listdir(OUT):
        os.remove(os.path.join(OUT, old))
    textures()
    models = [("wall_12", wall(12)), ("wall_6", wall(6)), ("fence_12", wall(12, 4.0, 0.5, "panel")), ("frame", frame()), ("door", door()),
              ("ground_yard", slab("floor", 52, 44)), ("ground_hall", slab("floor", 32, 32)), ("ground_core", slab("grate", 28, 28)),
              ("roof_hall", slab("panel", 32, 32, 0.5)), ("roof_core", slab("panel", 28, 28, 0.5)),
              ("container_blue", container("blue")), ("container_red", container("red")), ("container_green", container("green")),
              ("crate", crate()), ("barrel", barrel()), ("lamp", lamp()), ("drone", drone(False)), ("drone_heavy", drone(True)),
              ("rifle", rifle()), ("medkit", pickup("medkit")), ("ammo", pickup("ammo")), ("core", core()), ("node", node())]
    for name, mesh in models:
        A.write_obj(name, mesh, M, tile=4.0 if name.startswith(("ground", "roof")) else (0.5 if name in ("rifle", "drone", "drone_heavy") else 3.0))
    files = sorted(os.listdir(OUT))
    rs = "// Generated by tools/make_outpost_assets.py. Do not edit.\npub const FILES: &[(&str, &[u8])] = &[\n"
    rs += "".join(f'    ("{f}", include_bytes!("../template/outpost/{f}")),\n' for f in files) + "];\n"
    open(os.path.join(OUT, "..", "..", "src", "outpost_files.rs"), "w").write(rs)
    print(f"{len(files)} files, {sum(os.path.getsize(os.path.join(OUT, f)) for f in files) / 1024:.0f} KB")
