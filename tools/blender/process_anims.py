"""Procesa animaciones de Mixamo para el personaje: retarget a su pose de
reposo, corrección de brazos que atraviesan la cabeza y muestreo.

Se corre con Blender sin interfaz (no modifica ningún .blend):

    blender -b --factory-startup --python tools/blender/process_anims.py -- \\
        <personaje.blend> <assets/mono.json> <carpeta con .fbx> <salida.json> [--no-fix]

Para cada .fbx:

1. Retarget. Los FBX "sin piel" vienen con el esqueleto en pose T y los
   "con piel" en la pose de reposo del personaje (pose A). Se copia la
   orientación de cada hueso en cada cuadro y se recalculan las
   articulaciones con las longitudes del personaje, así sirven los dos.
2. Corrección de la cabeza. El mono tiene la cabeza enorme y muchas
   animaciones meten los brazos adentro. La forma exacta de la cabeza (con
   gorra y orejas) se convierte en una grilla 3D de "adentro" con un margen
   (número de vueltas generalizado, que funciona aunque la malla esté
   abierta en el cuello); en cada cuadro, si algún vértice de un
   brazo (brazo, antebrazo, mano y dedos, con el mismo skinning rígido que
   el motor) queda adentro, se gira el brazo desde el hombro lo mínimo
   necesario para sacarlo. Las correcciones se suavizan en el tiempo para
   que no haya saltos y se vuelve a verificar.
3. Salida: por cuadro, la rotación local de cada hueso respecto de su padre
   (cuaternión, ejes de Blender) y el desplazamiento de la cadera. La
   conversión al motor la hace `tools/build_assets.py`.
"""

import json
import math
import os
import sys

import bpy
import numpy as np
from mathutils import Matrix, Quaternion, Vector

SAMPLE_FPS = 15
# Grilla de la cabeza: tamaño de celda y margen (en celdas) alrededor de la malla.
HEAD_CELL = 0.015
HEAD_MARGIN_CELLS = 1
MAX_FIX_DEG = 80.0
# Vértices del brazo más cerca que esto del hombro no se revisan: girar el
# brazo no los mueve (los choques ahí son de la cabeza bajando, no del brazo).
SHOULDER_SKIP = 0.05
SMOOTH_SIGMA = 1.2   # en cuadros muestreados
LOOPING = ("idle", "walk", "jack", "push_up", "squat", "sitting_idle", "typing", "sleeping", "dance",
           "flair", "dizzy", "crying")

ARM_CHAIN = ("Arm", "ForeArm", "Hand", "HandThumb1", "HandThumb2", "HandThumb3", "HandThumb4",
             "HandIndex1", "HandIndex2", "HandIndex3", "HandIndex4")


def short(name):
    return name.split(":")[-1]


def load_target(blend_path, mono_json):
    """Reposo del personaje: cabeza y orientación de cada hueso (mundo)."""
    bpy.ops.wm.open_mainfile(filepath=blend_path, load_ui=False)
    arm = next(o for o in bpy.data.objects if o.type == "ARMATURE")
    rest = {}
    for b in arm.data.bones:
        m = arm.matrix_world @ b.matrix_local
        rest[b.name] = (m.to_3x3().normalized(), m.translation.copy())
    data = json.load(open(mono_json, encoding="utf-8"))
    bones = data["bones"]
    for b in bones:
        h = Vector(b["head"])
        if (rest[b["name"]][1] - h).length > 1e-3:
            raise SystemExit(f"el .blend no coincide con mono.json en el hueso {b['name']}")
    return data, rest


class Rig:
    def __init__(self, data, rest):
        self.bones = data["bones"]
        self.names = [b["name"] for b in self.bones]
        self.parent = [b["parent"] for b in self.bones]
        self.index = {n: i for i, n in enumerate(self.names)}
        self.rest_rot = [rest[n][0] for n in self.names]
        self.head = [Vector(b["head"]) for b in self.bones]
        verts = data["verts"]
        self.co = np.array([v["co"] for v in verts], dtype=np.float64)
        self.vbone = np.array([v["bone"] or 0 for v in verts])
        # Cabeza: grilla de "adentro" a partir de sus triángulos.
        hi = self.index[self.find("Head")]
        self.head_bone = hi
        tris = np.array([t["v"] for t in data["tris"]
                         if all(self.vbone[v] == hi for v in t["v"])])
        self.head_grid(tris)
        # Vértices de cada brazo.
        self.arms = {}
        for side in ("Left", "Right"):
            chain = [self.index[self.find(side + c)] for c in ARM_CHAIN if self.find(side + c, False)]
            far = np.linalg.norm(self.co - np.array(self.head[chain[0]]), axis=1) > SHOULDER_SKIP
            self.arms[side] = (chain[0], chain, np.isin(self.vbone, chain) & far)

    def head_grid(self, tris):
        tri = self.co[tris]                                     # (T, 3, 3)
        lo = tri.reshape(-1, 3).min(0) - 3 * HEAD_CELL
        hi = tri.reshape(-1, 3).max(0) + 3 * HEAD_CELL
        shape = np.ceil((hi - lo) / HEAD_CELL).astype(int) + 1
        axes = [lo[k] + HEAD_CELL * np.arange(shape[k]) for k in range(3)]
        pts = np.stack(np.meshgrid(*axes, indexing="ij"), -1).reshape(-1, 3)
        wind = np.zeros(len(pts))
        for start in range(0, len(pts), 2048):
            p = pts[start:start + 2048, None, :]
            a, b, c = (tri[None, :, k, :] - p for k in range(3))
            la, lb, lc = (np.linalg.norm(v, axis=-1) for v in (a, b, c))
            det = np.einsum("ijk,ijk->ij", a, np.cross(b, c))
            den = (la * lb * lc + np.einsum("ijk,ijk->ij", a, b) * lc
                   + np.einsum("ijk,ijk->ij", a, c) * lb + np.einsum("ijk,ijk->ij", b, c) * la)
            wind[start:start + 2048] = (2 * np.arctan2(det, den)).sum(1) / (4 * np.pi)
        inside = (np.abs(wind) > 0.5).reshape(shape)
        for _ in range(HEAD_MARGIN_CELLS):
            grown = inside.copy()
            for k in range(3):
                grown |= np.roll(inside, 1, k) | np.roll(inside, -1, k)
            inside = grown
        self.grid, self.grid_lo, self.grid_shape = inside, lo, shape
        self.head_cells = int(inside.sum())

    def inside_head(self, R, J, pts):
        """Qué puntos (pose actual) caen dentro de la cabeza."""
        hb = self.head_bone
        Rh = np.array(R[hb])
        rest = (pts - np.array(J[hb])) @ Rh + np.array(self.head[hb])   # R^T·(p - j) + h
        idx = np.rint((rest - self.grid_lo) / HEAD_CELL).astype(int)
        ok = np.all((idx >= 0) & (idx < self.grid_shape), axis=1)
        out = np.zeros(len(pts), bool)
        i = idx[ok]
        out[ok] = self.grid[i[:, 0], i[:, 1], i[:, 2]]
        return out

    def find(self, suffix, required=True):
        for n in self.names:
            if n.endswith(":" + suffix) or n == suffix:
                return n
        if required:
            raise SystemExit(f"falta el hueso {suffix}")
        return None

    def descendants(self, root):
        out = [root]
        for i, p in enumerate(self.parent):
            if p is not None and p in out:
                out.append(i)
        return out


def sample_clip(path, rig):
    """Rotación de mundo de cada hueso del personaje y articulaciones, por cuadro."""
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.fbx(filepath=path)
    src = next(o for o in bpy.data.objects if o.type == "ARMATURE")
    act = src.animation_data.action
    f0, f1 = (round(f) for f in act.frame_range)
    scene = bpy.context.scene
    src_fps = scene.render.fps / scene.render.fps_base
    step = src_fps / SAMPLE_FPS
    src_rest_hips = src.matrix_world @ src.data.bones[rig.names[0]].head_local
    frames = []
    t = float(f0)
    while t <= f1 + 1e-6:
        scene.frame_set(int(t), subframe=t - int(t))
        R, J = [], []
        for i, name in enumerate(rig.names):
            pb = src.pose.bones.get(name)
            if pb is None:
                raise SystemExit(f"{os.path.basename(path)}: falta el hueso {name}")
            m = src.matrix_world @ pb.matrix
            r = m.to_3x3().normalized() @ rig.rest_rot[i].transposed()
            p = rig.parent[i]
            if p is None:
                j = rig.head[i] + (m.translation - src_rest_hips)
            else:
                j = J[p] + R[p] @ (rig.head[i] - rig.head[p])
            R.append(r)
            J.append(j)
        frames.append((R, J))
        t += step
    return frames, (f0, f1)


def skin(rig, R, J, mask):
    """Posición de los vértices `mask` con skinning rígido (como el motor)."""
    idx = np.nonzero(mask)[0]
    out = np.empty((len(idx), 3))
    for k, vi in enumerate(idx):
        b = rig.vbone[vi]
        out[k] = R[b] @ Vector(rig.co[vi] - np.array(rig.head[b])) + J[b]
    return out


def penetration(rig, R, J, pts):
    """Cantidad de vértices dentro de la cabeza."""
    return int(rig.inside_head(R, J, pts).sum())


def rotation_vectors():
    dirs = []
    for x in (-1, 0, 1):
        for y in (-1, 0, 1):
            for z in (-1, 0, 1):
                if (x, y, z) != (0, 0, 0):
                    v = np.array([x, y, z], float)
                    dirs.append(v / np.linalg.norm(v))
    return dirs


DIRS = rotation_vectors()


def rot_matrix(axis, angle):
    return Matrix.Rotation(angle, 3, Vector(axis))


def needed_fix(rig, R, J, side, base_fix):
    """Rotación mínima (vector eje·ángulo) del brazo desde el hombro para que
    ningún vértice quede dentro de la cabeza, partiendo de `base_fix`."""
    root, chain, mask = rig.arms[side]
    shoulder = np.array(J[root])
    pts0 = skin(rig, R, J, mask)

    def apply(vec):
        ang = np.linalg.norm(vec)
        if ang < 1e-9:
            return pts0
        m = np.array(rot_matrix(vec / ang, ang))
        return (pts0 - shoulder) @ m.T + shoulder

    def depth(vec):
        return penetration(rig, R, J, apply(vec))

    if depth(base_fix) <= 0:
        return base_fix, 0
    limit = math.radians(MAX_FIX_DEG)
    best = None   # (vértices adentro, ángulo agregado, vector)
    for deg in np.arange(2.5, MAX_FIX_DEG + 0.1, 2.5):
        for d in DIRS:
            vec = base_fix + d * math.radians(deg)
            if np.linalg.norm(vec) > limit:
                continue
            cand = (depth(vec), deg, vec)
            if best is None or cand[:2] < best[:2]:
                best = cand
        if best is not None and best[0] == 0:
            break
    if best is None:
        return base_fix, depth(base_fix)
    return best[2], best[0]


def apply_fix(rig, R, J, side, vec):
    """Gira la cadena del brazo alrededor del hombro."""
    ang = np.linalg.norm(vec)
    if ang < 1e-9:
        return R, J
    C = rot_matrix(vec / ang, ang)
    root, chain, _ = rig.arms[side]
    s = J[root].copy()
    R, J = list(R), list(J)
    for i in rig.descendants(root):
        R[i] = C @ R[i]
        J[i] = s + C @ (J[i] - s)
    return R, J


def smooth(vecs, sigma):
    n = len(vecs)
    if n < 3 or sigma <= 0:
        return vecs
    k = np.arange(-3, 4)
    w = np.exp(-(k * k) / (2 * sigma * sigma))
    w /= w.sum()
    out = []
    for i in range(n):
        acc = np.zeros(3)
        for kk, ww in zip(k, w):
            acc += ww * vecs[min(n - 1, max(0, i + kk))]
        out.append(acc)
    return out


def worst(rig, frames):
    worst_d, hit = 0, 0
    for R, J in frames:
        d = sum(penetration(rig, R, J, skin(rig, R, J, rig.arms[side][2])) for side in ("Left", "Right"))
        worst_d = max(worst_d, d)
        hit += d > 0
    return worst_d, hit


def fix_clip(rig, frames):
    fixes = {s: [np.zeros(3) for _ in frames] for s in ("Left", "Right")}
    for _ in range(3):
        changed = False
        for side in ("Left", "Right"):
            for i, (R, J) in enumerate(frames):
                Rf, Jf = R, J
                other = "Right" if side == "Left" else "Left"
                Rf, Jf = apply_fix(rig, Rf, Jf, other, fixes[other][i])
                vec, _ = needed_fix(rig, Rf, Jf, side, fixes[side][i])
                if np.linalg.norm(vec - fixes[side][i]) > 1e-6:
                    changed = True
                fixes[side][i] = vec
            # Suavizar sin bajar de lo necesario: se suaviza y en la próxima
            # vuelta se completa lo que falte.
            fixes[side] = smooth(fixes[side], SMOOTH_SIGMA)
        if not changed:
            break
    # Pasada final sin suavizar para cerrar lo que el suavizado dejó abierto.
    out = []
    for i, (R, J) in enumerate(frames):
        for side in ("Left", "Right"):
            other = "Right" if side == "Left" else "Left"
            Rt, Jt = apply_fix(rig, R, J, other, fixes[other][i])
            vec, _ = needed_fix(rig, Rt, Jt, side, fixes[side][i])
            fixes[side][i] = vec
        Rn, Jn = apply_fix(rig, R, J, "Left", fixes["Left"][i])
        Rn, Jn = apply_fix(rig, Rn, Jn, "Right", fixes["Right"][i])
        out.append((Rn, Jn))
    max_deg = max(math.degrees(np.linalg.norm(v)) for s in fixes for v in fixes[s])
    return out, max_deg


def to_local(rig, frames):
    out = []
    for R, J in frames:
        quats = []
        for i, p in enumerate(rig.parent):
            q = (R[i] if p is None else R[p].transposed() @ R[i]).to_quaternion()
            quats.append(q)
        root = J[0] - rig.head[0]
        out.append((quats, root))
    # Continuidad del signo (para interpolar entre cuadros).
    for i in range(len(rig.names)):
        for f in range(1, len(out)):
            if out[f][0][i].dot(out[f - 1][0][i]) < 0:
                out[f][0][i].negate()
    return out


def main():
    args = sys.argv[sys.argv.index("--") + 1:]
    no_fix = "--no-fix" in args
    args = [a for a in args if not a.startswith("--")]
    blend, mono_json, folder, out_path = args[:4]
    only = set(args[4:])
    data, rest = load_target(blend, mono_json)
    rig = Rig(data, rest)
    print(f"cabeza: {rig.head_cells} celdas de {HEAD_CELL * 100:.1f} cm", flush=True)
    clips = []
    files = sorted(f for f in os.listdir(folder) if f.lower().endswith(".fbx"))
    for f in files:
        name = os.path.splitext(f)[0]
        name = name[5:] if name.startswith("anim_") else name
        if only and name not in only:
            continue
        frames, src_range = sample_clip(os.path.join(folder, f), rig)
        before = worst(rig, frames)
        max_fix = 0.0
        if not no_fix:
            frames, max_fix = fix_clip(rig, frames)
        after = worst(rig, frames)
        local = to_local(rig, frames)
        clips.append({
            "name": name,
            "file": f,
            "fps": SAMPLE_FPS,
            "loop": any(k in name for k in LOOPING) and "_to_" not in name and "from" not in name,
            "src_frames": list(src_range),
            "frames": [
                {"q": [[round(c, 5) for c in q] for q in quats], "root": [round(c, 5) for c in root]}
                for quats, root in local
            ],
            "head_before": {"max_verts": int(before[0]), "frames": int(before[1])},
            "head_after": {"max_verts": int(after[0]), "frames": int(after[1])},
            "max_fix_deg": round(max_fix, 1),
        })
        print(f"{name}: {len(frames)} cuadros, brazos en la cabeza antes {before[1]} cuadros "
              f"(hasta {before[0]} vértices), después {after[1]} (hasta {after[0]}), "
              f"corrección máx {max_fix:.0f}°", flush=True)
    out = {
        "bones": rig.names,
        "clips": clips,
    }
    json.dump(out, open(out_path, "w", encoding="utf-8"), separators=(",", ":"))


main()
