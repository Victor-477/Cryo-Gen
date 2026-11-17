#!/usr/bin/env python3
"""High-resolution first-person hands for Outpost Kestrel, built from code.

The hand is a signed distance field (palm, thenar pad, 15 phalanges as rounded cones, knuckles,
fingernails, a fingerless tactical glove with a knuckle guard), turned into a smooth mesh by marching
cubes at about 1.3 mm. Every finger has three joints; the fingers are curled automatically to wrap the
rifle's grip and handguard, and each vertex is skinned to the two nearest bones.

Used by make_outpost_assets.py (`hands_model()`); nothing here comes from an external asset.
"""
import math
import numpy as np
from skimage.measure import marching_cubes

# ------------------------------------------------------------------ small vector helpers
def unit(v):
    v = np.asarray(v, float)
    return v / (np.linalg.norm(v) or 1.0)

def rotate(v, axis, deg):
    """Rodrigues rotation of v about a unit axis."""
    a, t = unit(axis), math.radians(deg)
    v = np.asarray(v, float)
    return v * math.cos(t) + np.cross(a, v) * math.sin(t) + a * np.dot(a, v) * (1 - math.cos(t))

def quat_axis(axis, deg):
    a, h = unit(axis), math.radians(deg) / 2
    return (a[0] * math.sin(h), a[1] * math.sin(h), a[2] * math.sin(h), math.cos(h))

def smin(a, b, k):
    h = np.clip(0.5 + 0.5 * (b - a) / k, 0, 1)
    return b + (a - b) * h - k * h * (1 - h)

def round_cone(p, a, b, r1, r2):
    """Distance to a capsule whose radius goes from r1 at a to r2 at b (p: N x 3)."""
    ba = b - a
    l2 = np.dot(ba, ba)
    rr = r1 - r2
    a2 = l2 - rr * rr
    il2 = 1.0 / l2
    pa = p - a
    y = pa @ ba
    z = y - l2
    xv = pa * l2 - np.outer(y, ba)
    x2 = np.einsum("ij,ij->i", xv, xv)
    y2 = y * y * l2
    z2 = z * z * l2
    k = np.sign(rr) * rr * rr * x2
    out = (np.sqrt(x2 * a2 * il2) + y * rr) * il2 - r1
    m1 = np.sign(z) * a2 * z2 > k
    m2 = np.sign(y) * a2 * y2 < k
    out = np.where(m1, np.sqrt(x2 + z2) * il2 - r2, out)
    out = np.where(m2 & ~m1, np.sqrt(x2 + y2) * il2 - r1, out)
    return out

def round_box(p, center, axes, half, r):
    """Box with rounded edges; `axes` is a 3x3 matrix whose rows are the box's own x, y, z."""
    q = np.abs((p - center) @ np.asarray(axes).T) - (np.asarray(half) - r)
    return np.linalg.norm(np.maximum(q, 0), axis=1) + np.minimum(q.max(1), 0) - r

def ellipsoid(p, center, axes, radii):
    q = (p - center) @ np.asarray(axes).T
    k0 = np.linalg.norm(q / radii, axis=1)
    k1 = np.linalg.norm(q / (np.asarray(radii) ** 2), axis=1)
    return k0 * (k0 - 1) / np.maximum(k1, 1e-9)

# ------------------------------------------------------------------ the hand's anatomy (local frame)
# Local frame: wrist at the origin, fingers along +Y, palm facing +Z, thumb on +X for a right hand
# (side = +1) and on -X for a left hand (side = -1). Sizes are a man's hand in a thin glove, in metres.
FINGERS = [  # name, knuckle (x, y, z), splay in degrees, lengths, radii at knuckle .. tip
    ("Index", (0.029, 0.092, 0.0), 4.0, (0.044, 0.026, 0.021), (0.0102, 0.0094, 0.0086, 0.0078)),
    ("Middle", (0.009, 0.096, 0.0), 0.0, (0.048, 0.030, 0.022), (0.0104, 0.0096, 0.0088, 0.0079)),
    ("Ring", (-0.011, 0.093, 0.0), -3.0, (0.045, 0.028, 0.022), (0.0098, 0.009, 0.0083, 0.0075)),
    ("Pinky", (-0.029, 0.085, 0.0), -7.0, (0.036, 0.021, 0.019), (0.0086, 0.0079, 0.0072, 0.0066)),
]
THUMB_CMC = (0.022, 0.016, 0.006)
THUMB_LEN = (0.044, 0.032, 0.027)
THUMB_RAD = (0.0135, 0.0112, 0.0104, 0.0094)

class Chain:
    """One finger or thumb posed by forward kinematics: joint points, bone directions, palm-side
    vectors and the axis each joint bends about (all three bend in the finger's own plane)."""
    def __init__(self, name, base, d, n, lengths, radii, angles):
        self.name, self.radii, self.lengths = name, radii, lengths
        self.points, self.dirs, self.pads, self.axes = [np.asarray(base, float)], [], [], []
        d, n = unit(d), unit(n)
        for L, ang in zip(lengths, angles):
            a = unit(np.cross(d, n))     # rotating d about d x n turns it towards n: the finger curls
            d, n = rotate(d, a, ang), rotate(n, a, ang)
            self.axes.append(a)
            self.dirs.append(d)
            self.pads.append(n)
            self.points.append(self.points[-1] + d * L)

def pose(side, curls, thumb):
    """Builds the five chains. `curls`: finger name -> (mcp, pip, dip) degrees.
    `thumb`: (spread, lift, cmc flex, mcp, ip) degrees."""
    chains = []
    for name, (x, y, z), splay, lengths, radii in FINGERS:
        d = rotate((0, 1, 0), (0, 0, 1), -splay * side)
        chains.append(Chain(name, (x * side, y, z), d, (0, 0, 1), lengths, radii, curls[name]))
    spread, lift, flex, mcp, ip = thumb
    d = unit((0.55 * side, 0.75, 0.38))
    d = rotate(d, (0, 0, 1), -spread * side)
    d = rotate(d, unit(np.cross(d, (0, 0, 1))), lift)
    n = unit(np.cross(d, (0, 0, 1)) * -side)            # the thumb's pad looks across the palm
    n = unit(n + np.array([0, 0, 0.9]))
    n = unit(n - d * np.dot(n, d))
    chains.append(Chain("Thumb", (THUMB_CMC[0] * side, THUMB_CMC[1], THUMB_CMC[2]), d, n, THUMB_LEN, THUMB_RAD, (flex, mcp, ip)))
    return chains

# ------------------------------------------------------------------ distance field
GLOVE = 0.0013     # thickness of the glove fabric

def palm_sdf(p, side):
    x = np.array([1.0, 0, 0])
    body = round_box(p, np.array([0.0, 0.05, 0.0]), np.eye(3), (0.037, 0.05, 0.0125), 0.0105)
    body = smin(body, round_box(p, np.array([0.0, 0.088, 0.0]), np.eye(3), (0.043, 0.012, 0.012), 0.011), 0.012)
    # wrist: a flattened round section
    wrist = ellipsoid(p, np.array([0.0, -0.012, 0.0]), np.eye(3), (0.03, 0.03, 0.019))
    fore = round_cone(p * np.array([1, 1, 1.45]), np.array([0, -0.01, 0]), np.array([0, -0.07, 0]), 0.027, 0.03)
    body = smin(body, smin(wrist, fore, 0.01), 0.016)
    # hypothenar pad along the little-finger edge, palm side
    body = smin(body, round_cone(p, np.array([-0.028 * side, 0.015, 0.006]), np.array([-0.032 * side, 0.07, 0.004]), 0.013, 0.011), 0.01)
    # metacarpal heads (knuckles) on the back
    for name, (kx, ky, kz), *_ in FINGERS:
        body = smin(body, ellipsoid(p, np.array([kx * side, ky - 0.004, -0.004]), np.eye(3), (0.0105, 0.012, 0.0105)), 0.006)
    # palm creases: shallow grooves on the palm side
    crease = round_cone(p, np.array([0.035 * side, 0.075, 0.0135]), np.array([-0.038 * side, 0.068, 0.0125]), 0.0012, 0.0012)
    body = np.maximum(body, -(crease - 0.0003))
    return body

def chain_sdf(p, ch):
    d = None
    for i in range(3):
        seg = round_cone(p, ch.points[i], ch.points[i + 1], ch.radii[i], ch.radii[i + 1])
        d = seg if d is None else smin(d, seg, 0.0025)
    # fleshy pads on the palm side of each phalanx
    for i in range(3):
        mid = (ch.points[i] + ch.points[i + 1]) / 2 + ch.pads[i] * ch.radii[i] * 0.35
        pad = round_cone(p, mid - ch.dirs[i] * 0.004, mid + ch.dirs[i] * 0.004, ch.radii[i + 1] * 0.8, ch.radii[i + 1] * 0.8)
        d = smin(d, pad, 0.004)
    return d

def thenar_sdf(p, th):
    a, b = th.points[0] - th.dirs[0] * 0.006, th.points[1]
    return round_cone(p + 0, a, b, 0.0175, 0.0128)

def nail_sdf(p, ch):
    """A fingernail on the back of the last phalanx."""
    d, n = ch.dirs[2], ch.pads[2]
    side_axis = unit(np.cross(d, n))
    tip = ch.points[3]
    center = tip - d * (0.0072 + ch.radii[3] * 0.25) - n * (ch.radii[3] * 0.92)
    axes = np.array([side_axis, d, -n])
    w = ch.radii[3] * 0.74
    return round_box(p, center, axes, (w, 0.0068, 0.0006), 0.0005)

def coverage(p, chains):
    """1 where the glove covers the hand, 0 on bare skin: the glove stops halfway along the middle
    phalanx (the last joint of the thumb), with a small rolled hem."""
    best = np.full(len(p), np.inf)
    cov = np.ones(len(p))
    hem = np.zeros(len(p))
    for ch in chains:
        seg, frac = (1, 0.45) if ch.name != "Thumb" else (1, 0.62)
        cut = ch.points[seg] + ch.dirs[seg] * (ch.lengths[seg] * frac)
        dist = None
        for i in range(3):
            s = round_cone(p, ch.points[i], ch.points[i + 1], ch.radii[i], ch.radii[i + 1])
            dist = s if dist is None else np.minimum(dist, s)
        s = (p - cut) @ ch.dirs[seg]
        c = 1 - np.clip((s + 0.0016) / 0.0032, 0, 1)   # a soft edge: the fabric thins out over 3 mm
        h = np.clip(1 - np.abs(s + 0.0024) / 0.0022, 0, 1)
        mine = dist < best
        cov = np.where(mine, c, cov)
        hem = np.where(mine, h, hem)
        best = np.minimum(best, dist)
    palm_near = best > 0.012   # far from every finger: palm and wrist, always covered
    cov = np.where(palm_near, 1.0, cov)
    hem = np.where(palm_near, 0.0, hem)
    return cov, hem

FINGERS_LEN = {f[0]: f[3] for f in FINGERS}

def guard_sdf(p, side, chains):
    """Moulded knuckle guard over the back of the four knuckles."""
    k = np.array([c.points[0] for c in chains[:4]])
    center = k.mean(0) + np.array([0, -0.006, -0.0148])
    axes = np.eye(3)
    plate = round_box(p, center, axes, (0.041, 0.011, 0.0036), 0.0032)
    ribs = None
    for c in chains[:4]:
        r = ellipsoid(p, c.points[0] + np.array([0, -0.006, -0.0165]), axes, (0.0085, 0.0085, 0.0045))
        ribs = r if ribs is None else np.minimum(ribs, r)
    return smin(plate, ribs, 0.003)

def components(p, side, chains):
    palm = palm_sdf(p, side)
    thumb = chains[4]
    palm = smin(palm, thenar_sdf(p, thumb), 0.012)
    total = None
    for ch in chains:
        f = smin(chain_sdf(p, ch), palm, 0.0075 if ch.name != "Thumb" else 0.01)
        total = f if total is None else np.minimum(total, f)
    cov, hem = coverage(p, chains)
    hand = total - GLOVE * cov - 0.0007 * hem * cov
    nails = None
    for ch in chains:
        n = nail_sdf(p, ch)
        nails = n if nails is None else np.minimum(nails, n)
    guard = guard_sdf(p, side, chains)
    return hand, nails, guard, cov

def field(p, side, chains):
    hand, nails, guard, _ = components(p, side, chains)
    return np.minimum(smin(hand, nails, 0.0006), smin(hand, guard, 0.0015))

# ------------------------------------------------------------------ fitting the fingers to a grip
class Grip:
    """An elliptic cylinder the hand holds: centre point, axis, radius across `u` and across `v`."""
    def __init__(self, center, axis, u, ru, rv):
        self.c, self.a = np.asarray(center, float), unit(axis)
        self.u = unit(np.asarray(u, float) - self.a * np.dot(u, self.a))
        self.v = np.cross(self.a, self.u)
        self.ru, self.rv = ru, rv

    def surface_distance(self, pts):
        q = np.asarray(pts) - self.c
        x, y = q @ self.u, q @ self.v
        th = np.arctan2(y, x)
        re = self.ru * self.rv / np.sqrt((self.rv * np.cos(th)) ** 2 + (self.ru * np.sin(th)) ** 2)
        return np.hypot(x, y) - re

def fit_chain(build, grips, ranges, radius, target=None, weight_target=0.0):
    """Grid search over the joint angles so the chain lies on the grips without going into them."""
    best, best_e = None, np.inf
    for angles in ranges:
        ch = build(angles)
        pts, rad = [], []
        for i in range(3):
            for t in (0.35, 0.7, 1.0):
                pts.append(ch.points[i] + (ch.points[i + 1] - ch.points[i]) * t)
                rad.append(ch.radii[i] + (ch.radii[i + 1] - ch.radii[i]) * t + GLOVE)
        pts, rad = np.array(pts), np.array(rad)
        gap = np.min([g.surface_distance(pts) for g in grips], axis=0) - rad
        inside = np.minimum(gap, 0)
        e = np.sum(np.minimum(np.abs(gap), 0.02) ** 2 * np.linspace(0.6, 1.4, len(gap))) + 60 * np.sum(inside ** 2)
        e += 1e-7 * sum(a * a for a in angles)   # prefer gentle curls when the fit is equal
        if target is not None:
            e += weight_target * np.sum((ch.points[3] - ch.dirs[2] * 0.008 - target) ** 2)
        if e < best_e:
            best, best_e = angles, e
    return best

# ------------------------------------------------------------------ meshing and skinning
def mesh_hand(side, chains, R, t, joint_of, voxel=0.0013):
    """Marching cubes over the field; returns rifle-space triangles with normals, skins, uvs, materials."""
    pts = np.concatenate([np.array(c.points) for c in chains] + [np.array([[0, -0.075, 0], [0, 0.1, 0]])])
    lo, hi = pts.min(0) - 0.03, pts.max(0) + 0.03
    lo[1] = max(lo[1], -0.068)   # the sleeve covers the forearm
    shape = np.ceil((hi - lo) / voxel).astype(int) + 1
    gx, gy, gz = (lo[i] + np.arange(shape[i]) * voxel for i in range(3))
    vol = np.empty(shape, np.float32)
    for i, x in enumerate(gx):   # slice by slice keeps memory low
        Y, Z = np.meshgrid(gy, gz, indexing="ij")
        p = np.stack([np.full(Y.size, x), Y.ravel(), Z.ravel()], 1)
        vol[i] = field(p, side, chains).reshape(Y.shape)
    verts, faces, _, _ = marching_cubes(vol, 0.0, spacing=(voxel,) * 3)
    verts += lo
    keep = verts[:, 1] > -0.064
    # normals from the field's gradient: smooth, with no faceting
    e = voxel * 0.5
    grad = np.stack([field(verts + d, side, chains) - field(verts - d, side, chains) for d in np.eye(3) * e], 1)
    normals = grad / np.maximum(np.linalg.norm(grad, axis=1, keepdims=True), 1e-9)
    hand, nails, guard, cov = components(verts, side, chains)
    mat = np.where(nails < np.minimum(hand, guard) + 0.0002, "nail",
          np.where(guard < hand + 0.0003, "pad", np.where(cov > 0.5, "glove", "skin")))
    skins = skin_weights(verts, chains, joint_of)
    # wind every triangle so its face normal agrees with the field's gradient (outwards)
    fn = np.cross(verts[faces[:, 1]] - verts[faces[:, 0]], verts[faces[:, 2]] - verts[faces[:, 0]])
    flip = np.einsum("ij,ij->i", fn, normals[faces].mean(1)) < 0
    faces[flip] = faces[flip][:, ::-1]
    faces = faces[keep[faces].all(1)]
    world = verts @ R.T + t
    wn = normals @ R.T
    return world, wn, verts, faces, mat, skins

def skin_weights(verts, chains, joint_of):
    """Each vertex follows its two nearest bones, blended smoothly across the joints."""
    bones = [(np.array([0, -0.07, 0.0]), np.array([0, 0.0, 0.0]), joint_of["arm"]),
             (np.array([0, 0.0, 0.0]), np.array([0, 0.08, 0.0]), joint_of["hand"])]
    for ch in chains:
        for i in range(3):
            bones.append((ch.points[i], ch.points[i + 1], joint_of[(ch.name, i)]))
    D = np.stack([np.linalg.norm(verts - (a + np.outer(np.clip((verts - a) @ (b - a) / np.dot(b - a, b - a), 0, 1), b - a)), axis=1)
                  for a, b, _ in bones], 1)
    # the palm belongs to the hand, never to a neighbouring finger
    order = np.argsort(D, 1)
    j0, j1 = order[:, 0], order[:, 1]
    d0, d1 = D[np.arange(len(D)), j0], D[np.arange(len(D)), j1]
    w1 = np.clip(0.5 - (d1 - d0) / 0.012, 0, 0.5)
    ids = np.array([b[2] for b in bones])
    return list(zip(ids[j0], ids[j1], w1))

def box_uv(p, n, scale):
    a = np.abs(n)
    if a[0] >= a[1] and a[0] >= a[2]:
        return (p[2] / scale, p[1] / scale)
    if a[1] >= a[2]:
        return (p[0] / scale, p[2] / scale)
    return (p[0] / scale, p[1] / scale)

def add_to_mesh(m, world, wn, local, faces, mat, skins, uv_scale=0.08):
    for f in faces:
        mats = [mat[i] for i in f]
        mname = max(set(mats), key=mats.count)
        n = wn[f].mean(0)
        nl = np.cross(local[f[1]] - local[f[0]], local[f[2]] - local[f[0]])
        tri = []
        for i in f:
            sk = skins[i]
            tri.append((world[i], wn[i], (int(sk[0]), int(sk[1]), float(sk[2])), box_uv(local[i], nl, uv_scale)))
        m.tri(mname, *tri)

# ------------------------------------------------------------------ the two hands on the rifle
def frame(K, N):
    """Rotation whose columns are the hand's local X (across the knuckles), Y (fingers), Z (palm)."""
    K, N = unit(K), unit(N)
    N = unit(N - K * np.dot(N, K))
    F = np.cross(N, K)
    return np.stack([K, F, N], 1)

NAMES = ["Index", "Middle", "Ring", "Pinky", "Thumb"]

def angle_grid(*ranges):
    import itertools
    return list(itertools.product(*[np.arange(a, b + 1e-6, s) for a, b, s in ranges]))

def fit_hand(side, R, t, grips, trigger=None):
    """Curls every finger (and the thumb) around the grips, in the hand's local frame."""
    Rt = R.T
    local = [Grip(Rt @ (g.c - t), Rt @ g.a, Rt @ g.u, g.ru, g.rv) for g in grips]
    curls = {}
    def search(build, coarse, step, radii, target=None, w=0.0):
        """Coarse grid first, then a finer grid around the best coarse answer."""
        best = fit_chain(build, local, angle_grid(*coarse), radii, target, w)
        fine = [(b - step, b + step, step / 4) for b in best]
        fine = [(max(lo, c[0]), min(hi, c[1]), s) for (lo, hi, s), c in zip(fine, coarse)]
        return fit_chain(build, local, angle_grid(*fine), radii, target, w)
    for k, (name, base, splay, lengths, radii) in enumerate(FINGERS):
        def build(angles, name=name, k=k):
            c = {n: (0, 0, 0) for n in NAMES}
            c[name] = angles
            return pose(side, c, (0, 0, 0, 0, 0))[k]
        coarse = [(-10, 95, 10), (0, 105, 10), (0, 75, 10)]
        if name == "Index" and trigger is not None:
            curls[name] = search(build, coarse, 10, radii, Rt @ (trigger - t), 30.0)
        else:
            curls[name] = search(build, coarse, 10, radii)
    def build_thumb(a):
        return pose(side, {n: (0, 0, 0) for n in NAMES}, tuple(a))[4]
    thumb = search(build_thumb, [(-30, 45, 15), (-30, 45, 15), (0, 45, 15), (0, 60, 15), (0, 60, 15)], 15, THUMB_RAD)
    print(f"  side {side}: " + ", ".join(f"{n} {tuple(round(float(a)) for a in v)}" for n, v in curls.items()) + f", thumb {tuple(round(float(a)) for a in thumb)}")
    return curls, thumb

def build(Mesh, right, left, voxel=0.0013):
    """right/left: dict(R=rotation, t=wrist position, grips=[Grip], trigger=point or None, elbow=point,
    shoulder=point). Returns (mesh, joints, clips)."""
    m = Mesh()
    joints = [("Root", None, (0, 0, 0))]
    info = {}
    for label, side, h in (("Right", 1, right), ("Left", -1, left)):
        curls, thumb = fit_hand(side, h["R"], h["t"], h["grips"], h.get("trigger"))
        chains = pose(side, curls, thumb)
        R, t = h["R"], np.asarray(h["t"], float)
        arm = len(joints)
        joints.append((label + "Arm", 0, tuple(h["elbow"])))
        hand = len(joints)
        joints.append((label + "Hand", arm, tuple(t)))
        joint_of = {"arm": arm, "hand": hand}
        for ch in chains:
            for i in range(3):
                parent = hand if i == 0 else len(joints) - 1
                joint_of[(ch.name, i)] = len(joints)
                joints.append((f"{label}{ch.name}{i + 1}", parent, tuple(R @ ch.points[i] + t)))
        world, wn, local, faces, mat, skins = mesh_hand(side, chains, R, t, joint_of, voxel)
        add_to_mesh(m, world, wn, local, faces, mat, skins)
        # the sleeve: from inside the glove cuff to the elbow and on out of view
        wrist_in = R @ np.array([0, -0.04, 0]) + t
        wrist_out = R @ np.array([0, -0.09, 0]) + t
        elbow, shoulder = np.asarray(h["elbow"], float), np.asarray(h["shoulder"], float)
        path = [wrist_in, wrist_out, wrist_out + (elbow - wrist_out) * 0.5, elbow, elbow + (shoulder - elbow) * 0.5]
        radii = [0.036, 0.042, 0.05, 0.056, 0.064]
        sleeve(m, path, radii, arm)
        info[label] = dict(chains=chains, R=R, t=t, joint_of=joint_of)
    return m, joints, clips(info)

def sleeve(m, path, radii, joint, sides=28):
    """A fabric sleeve with soft folds, swept along a smooth path."""
    from make_ashen_assets import spline
    pts = []
    for i in range(len(path) - 1):
        for s in np.linspace(0, 1, 7, endpoint=False):
            pts.append(path[i] + (path[i + 1] - path[i]) * s)
    pts.append(path[-1])
    rads = np.interp(np.linspace(0, len(radii) - 1, len(pts)), np.arange(len(radii)), radii)
    rings, length = [], 0.0
    for i, p in enumerate(pts):
        tdir = unit(pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)])
        up = np.array([1.0, 0, 0]) if abs(tdir[1]) > 0.9 else np.array([0, 1.0, 0])
        u = unit(np.cross(tdir, up))
        v = np.cross(tdir, u)
        if i:
            length += np.linalg.norm(p - pts[i - 1])
        ring = []
        for k in range(sides + 1):
            a = k / sides * math.tau
            fold = 1 + 0.035 * math.sin(a * 3 + length * 40) * math.sin(length * 25 + 1.3) + 0.02 * math.sin(a * 7 - length * 31)
            d = math.cos(a) * u + math.sin(a) * v * 0.86
            ring.append((p + d * rads[i] * fold, unit(d), (joint, joint, 0.0), (k / sides * 2.0, length / 0.12)))
        rings.append(ring)
    for i in range(len(rings) - 1):
        for k in range(sides):
            m.quad("sleeve", rings[i][k], rings[i + 1][k], rings[i + 1][k + 1], rings[i][k + 1])

def clips(info):
    """Idle, Fire and Reload. Finger keys bend each joint about its own axis (quaternions)."""
    def curl(label, finger, amounts):
        h = info[label]
        ch = next(c for c in h["chains"] if c.name == finger)
        out = {}
        for i in range(3):
            axis = h["R"] @ ch.axes[i]
            out[h["joint_of"][(finger, i)]] = [(t, quat_axis(axis, a[i])) for t, a in amounts]
        return out
    def merge(*parts):
        o = {}
        for p in parts:
            o.update(p)
        return o
    rest = (0, 0, 0)
    breathe = [(0, rest), (1.0, (3, 2, 1)), (2.0, rest)]
    idle = merge(*(curl("Right", f, breathe) for f in ("Middle", "Ring", "Pinky")),
                 *(curl("Left", f, [(0, rest), (1.1, (2, 3, 2)), (2.0, rest)]) for f in ("Index", "Middle", "Ring", "Pinky")),
                 curl("Right", "Index", [(0, rest), (1.2, (1, 3, 2)), (2.0, rest)]))
    fire = merge(curl("Right", "Index", [(0, rest), (0.035, (7, 22, 12)), (0.12, rest)]))
    left = info["Left"]
    open_close = [(0, rest), (0.28, (-28, -30, -10)), (0.42, (12, 18, 10)), (0.95, (12, 18, 10)), (1.2, rest)]
    reload = merge(*(curl("Left", f, open_close) for f in ("Index", "Middle", "Ring", "Pinky")),
                   curl("Left", "Thumb", [(0, rest), (0.28, (-15, -20, -10)), (0.42, (5, 10, 10)), (0.95, (5, 10, 10)), (1.2, rest)]),
                   *(curl("Right", f, [(0, rest), (0.5, (-5, -6, -3)), (1.4, rest)]) for f in ("Middle", "Ring", "Pinky")))
    reload[("move", left["joint_of"]["arm"])] = [(0, (0, 0, 0)), (0.35, (0.14, -0.2, 0.26)), (0.6, (0.16, -0.32, 0.27)),
                                                (0.85, (0.15, -0.2, 0.27)), (1.15, (0.05, -0.05, 0.08)), (1.4, (0, 0, 0))]
    reload[left["joint_of"]["hand"]] = [(0, (0, 0, 0)), (0.35, (0, 0, -40)), (1.05, (0, 0, -40)), (1.4, (0, 0, 0))]
    return {"Idle": (2.0, idle), "Fire": (0.12, fire), "Reload": (1.4, reload)}
