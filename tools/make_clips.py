"""Genera animaciones que Mixamo no tiene, con cinemática inversa sobre una
animación base, para que las manos lleguen a puntos exactos del hábitat:

- eat_banana:   sentado a la mesa, lleva la banana del plato a la boca, muerde y vuelve.
- pick_banana:  parado frente al bananero, estira el brazo (con un saltito),
                arranca una banana y la baja.
- water_plants: parado, sostiene la regadera hacia la huerta y la inclina.
- read_book:    sentado, sostiene un libro con las dos manos y pasa la página.
- type_laptop:  el tipeo de Mixamo con las manos levantadas lo justo para que
                ningún dedo se hunda en la laptop (el mono tiene manos grandes).
- video_call:   sentado al escritorio, habla y gesticula con la derecha.
- think:        sentado, la mano en el mentón y el codo en el escritorio.
- sip_mate:     sentado, lleva el mate a la boca, toma y lo baja.
- stir:         parado frente a la olla, revuelve con la derecha.
- strum:        sentado, la izquierda en el mástil y la derecha rasguea.
- stretch:      parado, brazos a los costados y se inclina a un lado y al otro.
- phone_call:   en altavoz, el teléfono en la palma delante de la boca (encima de caminar).
- sweep:        las dos manos en la escoba, barriendo (encima de caminar).
- look_up:      parado, mira hacia arriba.
- gamepad:      sentado, el joystick con las dos manos y los pulgares inquietos.
- spray:        parado, el matafuego colgando de la derecha y la manguera en la
                izquierda apuntando a la olla.
- lie_down_side: sentado en el costado largo de la cama (mirando a la cámara),
                se recuesta de lado hacia la almohada, sube las piernas y se
                acomoda: termina exactamente en el primer cuadro de `sleeping`.
- get_up_side:  lo mismo al revés: baja las piernas, se incorpora y queda sentado.

    python tools/make_clips.py      # assets/mono_anims_extra.json

`tools/build_assets.py` las suma a las de Mixamo. Puntos en metros, ejes de
Blender, relativos al personaje (sus pies en el origen, mirando hacia -Y).
La boca se toma de la malla (las caras del material Mouth), así la banana
llega a la boca real.
"""

import json
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from preview_anims import ROOT, add, mm, mv, pose, qmat, sub  # noqa: E402
from build_assets import matrix_from_quat, quat_from_matrix  # noqa: E402

FPS = 15


def norm(v):
    n = math.sqrt(sum(x * x for x in v)) or 1.0
    return [x / n for x in v]


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def scale(v, s):
    return [x * s for x in v]


def transpose(m):
    return [[m[j][i] for j in range(3)] for i in range(3)]


def axis_angle(axis, ang):
    x, y, z = norm(axis)
    c, s, t = math.cos(ang), math.sin(ang), 1 - math.cos(ang)
    return [[t * x * x + c, t * x * y - s * z, t * x * z + s * y],
            [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
            [t * x * z - s * y, t * y * z + s * x, t * z * z + c]]


def swing(a, b):
    """Rotación mínima que lleva la dirección `a` a la dirección `b`."""
    a, b = norm(a), norm(b)
    ax = cross(a, b)
    s = math.sqrt(dot(ax, ax))
    c = max(-1.0, min(1.0, dot(a, b)))
    if s < 1e-9:
        return [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
    return axis_angle(ax, math.atan2(s, c))


def ease(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


def lerp3(a, b, t):
    return [a[k] + (b[k] - a[k]) * t for k in range(3)]


class Rig:
    def __init__(self, mono):
        self.mono = mono
        self.names = [b["name"].split(":")[-1] for b in mono["bones"]]
        self.i = {n: k for k, n in enumerate(self.names)}
        self.parent = [b["parent"] for b in mono["bones"]]
        # Vértices de cada mano (con sus dedos), para medir hasta dónde bajan.
        self.hand_verts = {}
        for side in ("Left", "Right"):
            bones = {k for k, n in enumerate(self.names) if n.startswith(side + "Hand")}
            self.hand_verts[side] = [(v["bone"], v["co"]) for v in mono["verts"] if v["bone"] in bones]
        mi = next(k for k, m in enumerate(mono["materials"]) if m["name"] == "Mouth")
        pts = [mono["verts"][v]["co"] for t in mono["tris"] if t["mat"] == mi for v in t["v"]]
        self.mouth_rest = [sum(p[k] for p in pts) / len(pts) for k in range(3)]

    def world(self, frame):
        return pose(self.mono, frame)

    def hand_low(self, R, J, side):
        """Altura del punto más bajo de la mano (con dedos) en esta pose."""
        return min(add(mv(R[b], sub(co, self.mono["bones"][b]["head"])), J[b])[2]
                   for b, co in self.hand_verts[side])

    def mouth(self, R, J):
        h = self.i["Head"]
        return add(mv(R[h], sub(self.mouth_rest, self.mono["bones"][h]["head"])), J[h])

    def ik(self, R, J, side, target, pole):
        """Lleva la muñeca (`<side>Hand`) a `target` girando brazo y
        antebrazo; `pole` es hacia dónde apunta el codo. Devuelve R y J nuevos."""
        a_i, f_i, h_i = self.i[side + "Arm"], self.i[side + "ForeArm"], self.i[side + "Hand"]
        S, E, H = J[a_i], J[f_i], J[h_i]
        la, lb = math.dist(S, E), math.dist(E, H)
        d_vec = sub(target, S)
        d = max(abs(la - lb) + 1e-4, min(la + lb - 1e-4, math.sqrt(dot(d_vec, d_vec))))
        u = norm(d_vec)
        v = sub(pole, scale(u, dot(pole, u)))
        v = norm(v if dot(v, v) > 1e-9 else [0, 0, -1])
        cos_a = (la * la + d * d - lb * lb) / (2 * la * d)
        sin_a = math.sqrt(max(0.0, 1 - cos_a * cos_a))
        E2 = add(S, add(scale(u, la * cos_a), scale(v, la * sin_a)))
        H2 = add(S, scale(u, d))
        r1 = swing(sub(E, S), sub(E2, S))
        r2 = swing(mv(r1, sub(H, E)), sub(H2, E2))
        R, J = list(R), list(J)
        chain = self.descendants(a_i)
        for k in chain:
            R[k] = mm(r1, R[k])
            J[k] = add(S, mv(r1, sub(J[k], S)))
        for k in self.descendants(f_i):
            R[k] = mm(r2, R[k])
            J[k] = add(E2, mv(r2, sub(J[k], E2)))
        return R, J

    def rotate_hand(self, R, J, side, axis, ang):
        h = self.i[side + "Hand"]
        rot = axis_angle(axis, ang)
        R, J = list(R), list(J)
        for k in self.descendants(h):
            R[k] = mm(rot, R[k])
            J[k] = add(J[h], mv(rot, sub(J[k], J[h])))
        return R, J

    def orient_hand(self, R, J, side, f_to, a_to):
        """Gira la mano (desde la muñeca) para que los dedos apunten a `f_to`
        y los nudillos (hacia el pulgar) lo más cerca posible de `a_to`."""
        f, _, a = self.hand_frame(side)
        h = self.i[side + "Hand"]
        f_to = norm(f_to)
        r1 = swing(mv(R[h], f), f_to)
        a1 = mv(r1, mv(R[h], a))
        a2 = sub(a_to, scale(f_to, dot(a_to, f_to)))
        rot = mm(swing(a1, a2), r1)
        R, J = list(R), list(J)
        for k in self.descendants(h):
            R[k] = mm(rot, R[k])
            J[k] = add(J[h], mv(rot, sub(J[k], J[h])))
        return R, J

    def orient_palm(self, R, J, side, f_to, n_to):
        """Como `orient_hand`, pero diciendo hacia dónde mira la palma."""
        s = 1 if side == "Right" else -1
        return self.orient_hand(R, J, side, f_to, scale(cross(norm(f_to), norm(n_to)), s))

    def rotate_bone(self, R, J, bone, axis, ang):
        """Gira `bone` y todo lo que cuelga de él alrededor de su articulación."""
        b = self.i[bone]
        rot = axis_angle(axis, ang)
        R, J = list(R), list(J)
        for k in self.descendants(b):
            R[k] = mm(rot, R[k])
            J[k] = add(J[b], mv(rot, sub(J[k], J[b])))
        return R, J

    def above(self, R, J, side, floor, pole):
        """Si la mano baja de `floor` (una mesa), la sube lo justo."""
        lift = floor - self.hand_low(R, J, side)
        if lift > 0:
            h = self.i[side + "Hand"]
            R, J = self.ik(R, J, side, add(J[h], [0, 0, lift]), pole)
        return R, J

    def hand_frame(self, side):
        """Ejes de la mano en reposo: `f` hacia los dedos, `n` hacia la palma
        (hacia donde se doblan los dedos) y `a` a lo largo de los nudillos,
        hacia el pulgar. Lo mismo calcula `habitat::grip` en el juego."""
        J = [b["head"] for b in self.mono["bones"]]
        g = lambda n: J[self.i[side + n]]
        h = g("Hand")
        f = norm(sub(g("HandIndex1"), h))
        d = sub(g("HandIndex4"), g("HandIndex2"))
        n = norm(sub(d, scale(f, dot(d, f))))
        a = norm(cross(f, n))
        if dot(a, sub(g("HandThumb2"), h)) < 0:
            a = scale(a, -1)
        return f, n, a

    def grip(self, R, J, side, curl):
        """Cierra la mano: los dedos (de reposo) se doblan hacia la palma
        `curl = (nudillo, medio, punta, pulgar)` grados, alrededor del eje
        de los nudillos de la pose actual."""
        bones = self.mono["bones"]
        R, J = list(R), list(J)
        fingers = [k for k, n in enumerate(self.names)
                   if n.startswith(side + "HandIndex") or n.startswith(side + "HandThumb")]
        for k in fingers:
            p = self.parent[k]
            R[k] = R[p]
            J[k] = add(J[p], mv(R[p], sub(bones[k]["head"], bones[p]["head"])))
        f, n, _ = self.hand_frame(side)
        h = self.i[side + "Hand"]
        axis = mv(R[h], cross(f, n))
        for joint, ang in zip(("HandIndex1", "HandIndex2", "HandIndex3"), curl[:3]):
            R, J = self.rotate_bone(R, J, side + joint, axis, math.radians(ang))
        # El pulgar se cierra sobre los dedos (alrededor del eje de los dedos).
        sign = 1 if side == "Right" else -1
        R, J = self.rotate_bone(R, J, side + "HandThumb1", mv(R[h], f), sign * math.radians(curl[3]))
        R, J = self.rotate_bone(R, J, side + "HandThumb2", axis, math.radians(curl[3] * 0.6))
        return R, J

    def descendants(self, root):
        out = [root]
        for k, p in enumerate(self.parent):
            if p is not None and p in out:
                out.append(k)
        return out

    def to_frame(self, R, J, root):
        q = []
        for k, p in enumerate(self.parent):
            m = R[k] if p is None else mm(transpose(R[p]), R[k])
            q.append(list(quat_from_matrix(m)))
        return {"q": q, "root": root}


def base_frames(anims, name):
    return next(c for c in anims["clips"] if c["name"] == name)["frames"]


# Manos que agarran algo: (nudillo, medio, punta, pulgar) en grados.
FIST = (78, 82, 60, 40)
HALF = (40, 45, 30, 20)


def build(rig, base, seconds, fn, loop=True, name="", grips=None):
    """`fn(t, R, J, root) -> (R, J, root)` por cuadro, `t` en segundos.
    `grips`: {"Right": FIST, ...} cierra esas manos (sostienen algo)."""
    n = round(seconds * FPS)
    frames = []
    for f in range(n):
        src = base[f % len(base)]
        R, J = rig.world(src)
        R, J, root = fn(f / FPS, R, J, list(src["root"]))
        for side, curl in (grips or {}).items():
            R, J = rig.grip(R, J, side, curl)
        frames.append(rig.to_frame(R, J, root))
    # Continuidad de signos para interpolar.
    for b in range(len(rig.names)):
        for f in range(1, n):
            if dot(frames[f]["q"][b], frames[f - 1]["q"][b]) < 0:
                frames[f]["q"][b] = [-x for x in frames[f]["q"][b]]
    return {"name": name, "fps": FPS, "loop": loop, "frames": frames}


def keyframes(t, keys):
    """Interpola puntos 3D entre (tiempo, punto) con suavizado."""
    if t <= keys[0][0]:
        return keys[0][1]
    for (t0, p0), (t1, p1) in zip(keys, keys[1:]):
        if t <= t1:
            return lerp3(p0, p1, ease((t - t0) / (t1 - t0)))
    return keys[-1][1]


def slerp(a, b, t):
    """Interpola cuaterniones (w, x, y, z) por el camino corto."""
    d = dot(a, b)
    if d < 0:
        b, d = [-x for x in b], -d
    if d > 0.9995:
        return norm([a[k] + (b[k] - a[k]) * t for k in range(4)])
    th = math.acos(min(1.0, d))
    sa, sb = math.sin((1 - t) * th), math.sin(t * th)
    return [(a[k] * sa + b[k] * sb) / math.sin(th) for k in range(4)]


def phase(t, t0, t1):
    return ease((t - t0) / (t1 - t0))


def lie_down_clips(rig, anims, spots):
    """Acostarse entrando por el costado de la cama, y levantarse.

    Las dos poses de los extremos: `sitting_idle` sentado en `spot_bed_side`
    y el primer cuadro de `sleeping` acostado en `spot_bed` (los dos lugares
    miran a la cámara, así los ejes del personaje son los del mundo). Se
    interpola la rotación de cada hueso **en el mundo**: el tronco rueda de
    costado hacia la almohada mientras las piernas siguen colgando, y después
    las piernas suben a la cama (y la cadera se corre hacia atrás).
    `lie_down_side` va relativo al asiento; `get_up_side`, a la cama, así el
    juego cambia de lugar justo cuando la pose de los dos coincide."""
    sit = base_frames(anims, "sitting_idle")[0]
    sleep = base_frames(anims, "sleeping")[0]
    seat = next(s for s in spots if s["name"] == "spot_bed_side")["pos"]
    bed = next(s for s in spots if s["name"] == "spot_bed")["pos"]
    shift = sub(bed, seat)
    R0, _ = rig.world(sit)
    R1, _ = rig.world(sleep)
    W0 = [quat_from_matrix(m) for m in R0]
    W1 = [quat_from_matrix(m) for m in R1]
    root0 = sit["root"]
    root1 = add(sleep["root"], shift)
    legs = {k for k, n in enumerate(rig.names) if any(p in n for p in ("UpLeg", "Leg", "Foot", "Toe"))}
    dur = 2.8

    def frame(t):
        roll = phase(t, 0.25, 1.45)      # el tronco cae de costado
        swing = phase(t, 1.05, 2.35)     # las piernas suben a la cama
        W = [slerp(W0[k], W1[k], swing if k in legs else roll) for k in range(len(W0))]
        R = [matrix_from_quat(q) for q in W]
        # La cadera sube un poco antes (las piernas pasan por encima del
        # colchón) y se corre hacia atrás con las piernas.
        lift = 0.035 * math.sin(math.pi * phase(t, 0.9, 2.5))
        root = [root0[0] + (root1[0] - root0[0]) * roll,
                root0[1] + (root1[1] - root0[1]) * swing,
                root0[2] + (root1[2] - root0[2]) * phase(t, 0.25, 1.6) + lift]
        return rig.to_frame(R, None, root)

    n = round(dur * FPS) + 1
    down = [frame(min(dur, f / FPS)) for f in range(n)]
    # Levantarse: al revés, relativo a la cama.
    up = []
    for f in reversed(down):
        g = {"q": f["q"], "root": sub(f["root"], shift)}
        up.append(g)
    for frames in (down, up):
        for b in range(len(rig.names)):
            for f in range(1, len(frames)):
                if dot(frames[f]["q"][b], frames[f - 1]["q"][b]) < 0:
                    frames[f]["q"][b] = [-x for x in frames[f]["q"][b]]
    return [{"name": "lie_down_side", "fps": FPS, "loop": False, "frames": down},
            {"name": "get_up_side", "fps": FPS, "loop": False, "frames": up}]


def main():
    mono = json.load(open(os.path.join(ROOT, "assets", "mono.json"), encoding="utf-8"))
    anims = json.load(open(os.path.join(ROOT, "assets", "mono_anims.json"), encoding="utf-8"))
    rig = Rig(mono)
    clips = []

    # Comer: plato -> boca -> morder -> abajo. Pole: codo hacia afuera y abajo.
    sit = base_frames(anims, "sitting_idle")
    plate = [-0.07, -0.23, 0.29]
    rest = [-0.11, -0.2, 0.27]

    def eat(t, R, J, root):
        mouth = rig.mouth(R, J)
        bite = add(mouth, [-0.035, -0.05, -0.075])
        chew = 0.012 * math.sin(t * 2 * math.pi * 3)
        keys = [(0.0, plate), (0.35, plate), (0.95, bite), (1.15, add(bite, [0, -0.015, -0.01])),
                (1.35, bite), (1.55, add(bite, [0, -0.015, -0.01])), (1.75, bite), (2.35, rest), (2.6, plate)]
        tgt = keyframes(t % 2.6, keys)
        if 0.95 <= t % 2.6 <= 1.75:
            tgt = add(tgt, [0, 0, chew])
        R, J = rig.ik(R, J, "Right", tgt, [-0.7, 0.3, -0.65])
        return R, J, root

    clips.append(build(rig, sit, 2.6, eat, name="eat_banana", grips={"Right": FIST}))

    # Cosechar: estira el brazo hacia arriba y adelante con un saltito.
    idle = base_frames(anims, "idle")
    low = [-0.17, 0.0, 0.32]
    reach = [-0.07, -0.05, 0.72]

    def pick(t, R, J, root):
        keys = [(0.0, low), (0.5, add(low, [0.02, -0.04, 0.1])), (1.0, reach), (1.5, reach),
                (1.9, add(reach, [0, 0.0, -0.06])), (2.6, [-0.12, -0.12, 0.36]), (3.0, [-0.12, -0.12, 0.36])]
        tgt = keyframes(t, keys)
        hop = 0.045 * math.sin(math.pi * max(0.0, min(1.0, (t - 0.85) / 0.6)))
        R, J = rig.ik(R, J, "Right", sub(tgt, [0, 0, hop]), [-0.8, 0.4, -0.3])
        root = add(root, [0, 0, hop])
        return R, J, root

    clips.append(build(rig, idle, 3.0, pick, loop=False, name="pick_banana", grips={"Right": HALF}))

    # Regar: la regadera colgando adelante de la manija (los dedos hacia
    # abajo y el pulgar hacia adelante, como el pico), y de a ratos la
    # inclina hacia adelante para volcar el agua.
    def water(t, R, J, root):
        tgt = [-0.14, -0.1, 0.4]
        R, J = rig.ik(R, J, "Right", tgt, [-0.8, 0.3, -0.5])
        R, J = rig.orient_hand(R, J, "Right", [0, -0.25, -1], [0, -1, 0])
        tilt = math.radians(35) * ease((math.sin(t * 2 * math.pi / 3.0) + 1) / 2)
        R, J = rig.rotate_hand(R, J, "Right", [1, 0, 0], tilt)
        return R, J, root

    clips.append(build(rig, idle, 6.0, water, name="water_plants", grips={"Right": FIST}))

    # Leer: el libro con las dos manos, tomado de los costados (las manos un
    # poco más afuera que sus bordes, a ±0,10 m); cada 4 s la derecha pasa la
    # página por delante de las hojas (más cerca de la cara que el libro).
    def read(t, R, J, root):
        lt = [0.115, -0.25, 0.37]
        rt = [-0.115, -0.25, 0.37]
        ph = t % 4.0
        if 3.0 <= ph:
            k = math.sin(math.pi * (ph - 3.0))
            rt = add(rt, [0.13 * k, 0.05 * k, 0.04 * k])
        R, J = rig.ik(R, J, "Left", lt, [0.8, 0.3, -0.5])
        R, J = rig.ik(R, J, "Right", rt, [-0.8, 0.3, -0.5])
        return R, J, root

    clips.append(build(rig, sit, 8.0, read, name="read_book", grips={"Left": HALF, "Right": HALF}))

    # Programar: el tipeo de Mixamo, con cada mano subida lo que haga falta
    # para que su punto más bajo quede sobre el teclado (base de la laptop a
    # 0,247 m: escritorio de 0,235 + 12 mm).
    typing = base_frames(anims, "sitting_typing_computer")
    keys = 0.250

    def type_laptop(t, R, J, root):
        for side, pole in (("Left", [0.8, 0.3, -0.5]), ("Right", [-0.8, 0.3, -0.5])):
            lift = keys - rig.hand_low(R, J, side)
            if lift > 0:
                h = rig.i[side + "Hand"]
                # El codo sigue apuntando para donde apuntaba.
                elbow = J[rig.i[side + "ForeArm"]]
                mid = [(J[rig.i[side + "Arm"]][k] + J[h][k]) / 2 for k in range(3)]
                pole = sub(elbow, mid)
                R, J = rig.ik(R, J, side, add(J[h], [0, 0, lift]), pole)
        return R, J, root

    clips.append(build(rig, typing, len(typing) / FPS, type_laptop, name="type_laptop"))

    L_POLE, R_POLE = [0.8, 0.3, -0.5], [-0.8, 0.3, -0.5]
    desk = 0.24  # escritorio (0,235) + 5 mm

    # Reunión: gesticula con la derecha, la izquierda apoyada en el escritorio.
    def video_call(t, R, J, root):
        a = 2 * math.pi * t / 2.5
        rt = [-0.1 + 0.035 * math.cos(a), -0.25, 0.33 + 0.03 * math.sin(2 * a)]
        if 4.0 <= t % 8.0 < 5.0:  # levanta la mano para pedir la palabra
            rt = add(rt, [0.0, 0.0, 0.08 * math.sin(math.pi * (t % 1.0))])
        R, J = rig.ik(R, J, "Left", [0.1, -0.23, 0.27], L_POLE)
        R, J = rig.ik(R, J, "Right", rt, R_POLE)
        R, J = rig.above(R, J, "Left", desk, L_POLE)
        R, J = rig.above(R, J, "Right", desk, R_POLE)
        return R, J, root

    clips.append(build(rig, sit, 8.0, video_call, name="video_call"))

    # Pensando: la mano derecha en el mentón (debajo de la boca), la otra en
    # el escritorio, que de vez en cuando tamborilea.
    def think(t, R, J, root):
        chin = add(rig.mouth(R, J), [-0.01, 0.02, -0.08])
        tap = 0.01 * max(0.0, math.sin(2 * math.pi * t * 3)) if t % 6.0 > 4.5 else 0.0
        R, J = rig.ik(R, J, "Right", chin, [-0.6, 0.3, -0.8])
        R, J = rig.ik(R, J, "Left", [0.11, -0.23, 0.27 + tap], L_POLE)
        R, J = rig.above(R, J, "Left", desk, L_POLE)
        return R, J, root

    clips.append(build(rig, sit, 6.0, think, name="think"))

    # Mate: del apoyabrazos a la boca, toma (2 s) y lo baja.
    def sip(t, R, J, root):
        mouth = rig.mouth(R, J)
        rest = [-0.12, -0.17, 0.26]
        sip_at = add(mouth, [-0.03, -0.05, -0.06])
        tgt = keyframes(t % 7.0, [(0.0, rest), (1.5, rest), (2.3, sip_at), (4.3, sip_at), (5.2, rest), (7.0, rest)])
        R, J = rig.ik(R, J, "Right", tgt, [-0.7, 0.3, -0.65])
        R, J = rig.ik(R, J, "Left", [0.12, -0.15, 0.25], L_POLE)
        return R, J, root

    clips.append(build(rig, sit, 7.0, sip, name="sip_mate", grips={"Right": FIST}))

    # Revolver: la derecha da vueltas sobre la olla (adelante, a la altura
    # del pecho: la mesada le llega al pecho), la izquierda en la mesada.
    def stir(t, R, J, root):
        a = 2 * math.pi * t / 1.5
        # La olla, 0,22 m adelante del hombro derecho; la cuchara cuelga
        # 0,15 m del puño.
        rt = [-0.12 + 0.025 * math.cos(a), -0.22 + 0.025 * math.sin(a), 0.58]
        R, J = rig.ik(R, J, "Right", rt, [-0.8, 0.2, -0.4])
        # El puño con el pulgar arriba: la cuchara baja derecha a la olla.
        R, J = rig.orient_hand(R, J, "Right", [0.2, -1, -0.35], [0, 0, 1])
        R, J = rig.ik(R, J, "Left", [0.12, -0.16, 0.48], [0.8, 0.2, -0.4])
        return R, J, root

    clips.append(build(rig, idle, 6.0, stir, name="stir", grips={"Right": FIST}))

    # Guitarra: la izquierda en el mástil, la derecha rasguea sobre la boca.
    def strum(t, R, J, root):
        k = math.sin(2 * math.pi * t * 2)
        rt = [-0.03, -0.17, 0.27 + 0.03 * k]
        lt = [0.2, -0.17, 0.33 + 0.004 * math.sin(2 * math.pi * t / 2)]
        R, J = rig.ik(R, J, "Right", rt, [-0.8, 0.5, -0.4])
        R, J = rig.ik(R, J, "Left", lt, [0.6, 0.6, -0.6])
        return R, J, root

    clips.append(build(rig, sit, 4.0, strum, name="strum", grips={"Left": FIST, "Right": HALF}))

    # Elongar: brazos abiertos a los costados (no le llegan por encima de la
    # cabeza) y se inclina a un lado y al otro desde la cintura.
    def stretch(t, R, J, root):
        R, J = rig.ik(R, J, "Left", [0.36, -0.02, 0.52], [0.0, 0.3, -1.0])
        R, J = rig.ik(R, J, "Right", [-0.36, -0.02, 0.52], [0.0, 0.3, -1.0])
        bend = math.radians(18) * math.sin(2 * math.pi * t / 6.0)
        R, J = rig.rotate_bone(R, J, "Spine", [0, 1, 0], bend)
        return R, J, root

    clips.append(build(rig, idle, 6.0, stretch, name="stretch"))

    # Teléfono: en altavoz, en la palma delante de la boca (la mano del mono
    # es enorme: contra la oreja el teléfono no se vería). Lo acerca para
    # hablar y lo baja un poco para escuchar; la cabeza asiente a ratos.
    def phone(t, R, J, root):
        talk = 0.5 + 0.5 * math.sin(2 * math.pi * t / 4.0)
        at = add(rig.mouth(R, J), [-0.05, -0.07 - 0.03 * talk, -0.09 - 0.03 * (1 - talk)])
        R, J = rig.ik(R, J, "Right", at, [-0.8, 0.3, -0.5])
        R, J = rig.orient_palm(R, J, "Right", [0.25, -1, 0.35], [0, 0.55, 0.85])
        nod = math.radians(6) * max(0.0, math.sin(2 * math.pi * t / 1.2)) if t % 4.0 < 1.2 else 0.0
        R, J = rig.rotate_bone(R, J, "Head", [1, 0, 0], nod)
        return R, J, root

    clips.append(build(rig, idle, 4.0, phone, name="phone_call", grips={"Right": HALF}))

    # Barrer: las dos manos en el palo (la izquierda arriba), de lado a lado.
    def sweep(t, R, J, root):
        k = math.sin(2 * math.pi * t / 1.0)
        lt = [0.03 + 0.03 * k, -0.15, 0.45]
        rt = [-0.03 + 0.07 * k, -0.2, 0.29]
        R, J = rig.ik(R, J, "Left", lt, [0.8, 0.3, -0.4])
        R, J = rig.ik(R, J, "Right", rt, [-0.8, 0.3, -0.4])
        return R, J, root

    clips.append(build(rig, idle, 2.0, sweep, name="sweep", grips={"Left": FIST, "Right": FIST}))

    # Mirar las estrellas: la cabeza y el cuello hacia arriba, de a ratos
    # mira para un lado.
    def look_up(t, R, J, root):
        R, J = rig.rotate_bone(R, J, "Neck", [1, 0, 0], math.radians(-14))
        R, J = rig.rotate_bone(R, J, "Head", [1, 0, 0], math.radians(-16))
        side = math.radians(20) * math.sin(2 * math.pi * t / 10.0)
        R, J = rig.rotate_bone(R, J, "Head", [0, 0, 1], side)
        return R, J, root

    clips.append(build(rig, idle, 10.0, look_up, name="look_up"))

    # Consola: las dos manos juntas adelante de la panza, pulgares inquietos.
    def gamepad(t, R, J, root):
        j = 0.008 * math.sin(2 * math.pi * t * 3.3)
        R, J = rig.ik(R, J, "Left", [0.08, -0.22, 0.33 + j], L_POLE)
        R, J = rig.ik(R, J, "Right", [-0.08, -0.22, 0.33 - j], R_POLE)
        lean = math.radians(5) * math.sin(2 * math.pi * t / 3.0)
        R, J = rig.rotate_bone(R, J, "Spine", [0, 1, 0], lean)
        return R, J, root

    clips.append(build(rig, sit, 3.0, gamepad, name="gamepad", grips={"Left": HALF, "Right": HALF}))

    # Matafuego: la derecha lo sostiene de la manija (colgando, como la
    # regadera) y la izquierda apunta la manguera a la olla, sacudiéndola.
    def spray(t, R, J, root):
        shake = 0.012 * math.sin(2 * math.pi * t * 5)
        R, J = rig.ik(R, J, "Right", [-0.13, -0.15, 0.4], [-0.8, 0.3, -0.5])
        R, J = rig.orient_hand(R, J, "Right", [0.1, -0.35, -1], [0, -1, 0])
        R, J = rig.ik(R, J, "Left", [-0.02 + shake, -0.27, 0.45 + shake], [0.8, 0.3, -0.5])
        R, J = rig.orient_palm(R, J, "Left", [-0.3, -1, 0.1], [-1, 0, 0])
        return R, J, root

    clips.append(build(rig, idle, 2.0, spray, name="spray", grips={"Right": FIST, "Left": HALF}))

    house = json.load(open(os.path.join(ROOT, "assets", "house.json"), encoding="utf-8"))
    clips += lie_down_clips(rig, anims, house["spots"])

    out = {"bones": [b["name"] for b in mono["bones"]], "clips": clips}
    path = os.path.join(ROOT, "assets", "mono_anims_extra.json")
    json.dump(out, open(path, "w", encoding="utf-8"), separators=(",", ":"))
    for c in clips:
        print(f"{c['name']}: {len(c['frames'])} cuadros")
    print("->", os.path.relpath(path, ROOT))


if __name__ == "__main__":
    main()
