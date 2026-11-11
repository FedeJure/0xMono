"""Mapa de navegación del hábitat, a partir de la escena exportada.

1. Grilla de 5 cm vista desde arriba: es obstáculo todo lo que tenga caras
   entre 3 cm y 95 cm de altura (lo que el mono choca), salvo pisos y
   alfombras; fuera del piso también (y la franja de adelante de la casa,
   pegada a la cámara).
2. Los obstáculos se agrandan por el radio del mono: así basta con mover su
   centro por celdas libres.
3. Red de nodos: puntos de paso, lugares de la escena y una malla de puntos
   libres, unidos cuando la recta entre ellos no cruza nada. En el juego el
   camino se busca en esta red (pocos nodos, poca RAM) y después se estira
   ("string pulling") con la grilla.

Los asientos con mesa adelante (escritorio, comedor) tienen silla que se
corre hacia atrás para sentarse: `SEATS` dice cuál y cuánto.

Lo usa `build_assets.py`; `nav_preview.py` lo dibuja.
"""

import math
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

CELL = 0.05
RADIUS = 0.17
BAND = (0.03, 0.95)
NODE_STEP = 0.30
LINK = 0.75
# Objetos que no estorban: pisos, alfombras, cosas altas o lejanas.
WALKABLE = {"floor", "garden_ground", "garden_path", "rug_bedroom", "rug_office", "rug_dining",
            "yoga_mat", "roof", "far_hills", "frame", "pendant_lamp"}
# Lo que no cuenta (se mueve o aparece según el estado).
SKIP_KINDS = ("prop",)
# Regiones donde se puede caminar (x0, x1, y0, y1) en metros de Blender. La
# franja de adelante de la casa queda afuera: ahí el mono taparía la cámara.
FLOORS = [(-3.70, 3.70, 0.30, 2.70), (3.60, 3.80, 1.15, 1.95), (3.70, 7.90, 0.55, 3.60)]
# Alturas que estorban por objeto: las hojas del bananero se pueden rozar,
# solo choca con el tronco.
BAND_OF = {"banana_tree": (0.03, 0.35)}
# Asiento -> (silla que se corre, cuánto hacia atrás).
SEATS = {
    "spot_desk": ("desk_chair", 0.30),
    "spot_table": ("dining_chair", 0.30),
    "spot_armchair": (None, 0.0),
    "spot_bed_side": (None, 0.0),
    "spot_bed_guitar": (None, 0.0),
}
# Cuánto adelante del asiento queda parado al empezar a sentarse
# (`stand_to_sit` alineado con `sitting_idle`; ver habitat.rs).
STAND_AHEAD = 0.214


def build_nav(scene):
    xs = [v[0] for o in scene["objects"] for v in o["verts"]]
    ys = [v[1] for o in scene["objects"] for v in o["verts"]]
    x0 = min(r[0] for r in FLOORS)
    x1 = max(r[1] for r in FLOORS)
    y0 = min(r[2] for r in FLOORS)
    y1 = max(r[3] for r in FLOORS)
    del xs, ys
    w = math.ceil((x1 - x0) / CELL)
    h = math.ceil((y1 - y0) / CELL)

    def cell_of(x, y):
        return int((x - x0) / CELL), int((y - y0) / CELL)

    def center(i, j):
        return x0 + (i + 0.5) * CELL, y0 + (j + 0.5) * CELL

    solid = [[True] * w for _ in range(h)]
    for j in range(h):
        for i in range(w):
            x, y = center(i, j)
            if any(a <= x <= b and c <= y <= d for a, b, c, d in FLOORS):
                solid[j][i] = False

    def raster_tri(pts):
        # Proyección del triángulo: todas las celdas cuyo centro cae adentro,
        # más las que toca cada vértice y arista (los triángulos verticales
        # de las paredes se proyectan como segmentos).
        def mark(x, y):
            i, j = cell_of(x, y)
            if 0 <= i < w and 0 <= j < h:
                solid[j][i] = True

        for (ax, ay), (bx, by) in zip(pts, pts[1:] + pts[:1]):
            n = max(1, int(math.dist((ax, ay), (bx, by)) / (CELL / 2)))
            for k in range(n + 1):
                t = k / n
                mark(ax + (bx - ax) * t, ay + (by - ay) * t)
        (ax, ay), (bx, by), (cx, cy) = pts
        det = (bx - ax) * (cy - ay) - (cx - ax) * (by - ay)
        if abs(det) < 1e-9:
            return
        i0, j0 = cell_of(min(ax, bx, cx), min(ay, by, cy))
        i1, j1 = cell_of(max(ax, bx, cx), max(ay, by, cy))
        for j in range(max(0, j0), min(h, j1 + 1)):
            for i in range(max(0, i0), min(w, i1 + 1)):
                px, py = center(i, j)
                l1 = ((bx - px) * (cy - py) - (cx - px) * (by - py)) / det
                l2 = ((cx - px) * (ay - py) - (ax - px) * (cy - py)) / det
                if l1 >= 0 and l2 >= 0 and l1 + l2 <= 1:
                    solid[j][i] = True

    for o in scene["objects"]:
        if o["name"] in WALKABLE or o["kind"] in SKIP_KINDS or o["kind"].startswith("state:"):
            continue
        vs = o["verts"]
        lo, hi = BAND_OF.get(o["name"], BAND)
        for t in o["tris"]:
            p = [vs[k] for k in t["v"]]
            if max(q[2] for q in p) < lo or min(q[2] for q in p) > hi:
                continue
            raster_tri([(q[0], q[1]) for q in p])

    # Agrandar por el radio del mono.
    r = math.ceil(RADIUS / CELL)
    offs = [(di, dj) for dj in range(-r, r + 1) for di in range(-r, r + 1)
            if (di * di + dj * dj) * CELL * CELL <= RADIUS * RADIUS]
    blocked = [[False] * w for _ in range(h)]
    for j in range(h):
        for i in range(w):
            if solid[j][i]:
                for di, dj in offs:
                    ii, jj = i + di, j + dj
                    if 0 <= ii < w and 0 <= jj < h:
                        blocked[jj][ii] = True

    def free(x, y):
        i, j = cell_of(x, y)
        return 0 <= i < w and 0 <= j < h and not blocked[j][i]

    def clear(a, b):
        # Más estricto que el juego (que muestrea cada media celda): muestras
        # cada medio centímetro en una franja de ±2 mm, así el redondeo a Q8
        # nunca lo hace pisar una celda bloqueada.
        d = math.dist(a, b)
        n = max(1, int(d / (CELL / 10)))
        nx, ny = ((a[1] - b[1]) / d, (b[0] - a[0]) / d) if d > 0 else (0, 0)
        for k in range(n + 1):
            x, y = a[0] + (b[0] - a[0]) * k / n, a[1] + (b[1] - a[1]) * k / n
            for o in (-0.002, 0.0, 0.002):
                if not free(x + nx * o, y + ny * o):
                    return False
        return True

    def snap(p):
        i, j = cell_of(*p)
        return center(i, j)

    def nearest_free(p, avoid=None):
        best = None
        for j in range(h):
            for i in range(w):
                if blocked[j][i]:
                    continue
                c = center(i, j)
                if avoid and avoid[0] <= c[0] <= avoid[1] and avoid[2] <= c[1] <= avoid[3]:
                    continue
                d = math.dist(c, p)
                if best is None or d < best[0]:
                    best = (d, c)
        return best[1]

    # Nodos con nombre: puntos de paso y lugares. Si el lugar está dentro de
    # un mueble (asientos, cama), su nodo es el punto libre más cercano.
    nodes, names = [], []
    seats = []
    for s in scene["waypoints"] + scene["spots"]:
        p = (s["pos"][0], s["pos"][1])
        name = s["name"]
        if name in SEATS:
            chair, pull = SEATS[name]
            yaw = math.radians(s["yaw"])
            # Hacia adelante del personaje (mira a -Y con yaw 0).
            fwd = (math.sin(yaw), -math.cos(yaw))
            seat_out = (p[0] - fwd[0] * pull, p[1] - fwd[1] * pull)
            stand = (seat_out[0] + fwd[0] * STAND_AHEAD, seat_out[1] + fwd[1] * STAND_AHEAD)
            seats.append({"name": name, "chair": chair, "pull": pull, "seat": p, "stand": stand})
            # Se acerca por un costado: ni donde estaba la silla ni por donde
            # se corre (agrandado por su radio).
            avoid = None
            if chair:
                vs = [v for o in scene["objects"] if o["name"] == chair for v in o["verts"]]
                bx = [min(v[0] for v in vs), max(v[0] for v in vs)]
                by = [min(v[1] for v in vs), max(v[1] for v in vs)]
                for k in range(2):
                    bx.append(bx[k] - fwd[0] * pull)
                    by.append(by[k] - fwd[1] * pull)
                avoid = (min(bx) - RADIUS, max(bx) + RADIUS, min(by) - RADIUS, max(by) + RADIUS)
            p = stand
            p = nearest_free(p, avoid)
        p = snap(p) if free(*p) else nearest_free(p)
        nodes.append(p)
        if name in SEATS:
            seats[-1]["approach"] = p
        names.append(name)
    # Malla de puntos libres.
    k = 0
    yy = y0 + NODE_STEP / 2
    while yy < y1:
        xx = x0 + NODE_STEP / 2 + (NODE_STEP / 2 if k % 2 else 0)
        while xx < x1:
            if free(xx, yy) and all(math.dist((xx, yy), n) > NODE_STEP * 0.6 for n in nodes):
                nodes.append(snap((xx, yy)))
                names.append(None)
            xx += NODE_STEP
        yy += NODE_STEP * 0.87
        k += 1
    edges = []
    for a in range(len(nodes)):
        for b in range(a + 1, len(nodes)):
            if math.dist(nodes[a], nodes[b]) <= LINK and clear(nodes[a], nodes[b]):
                edges.append((a, b))

    # Todo conectado: si no, algún lugar quedaría inalcanzable.
    adj = {i: [] for i in range(len(nodes))}
    for a, b in edges:
        adj[a].append(b)
        adj[b].append(a)
    seen, todo = {0}, [0]
    while todo:
        for m in adj[todo.pop()]:
            if m not in seen:
                seen.add(m)
                todo.append(m)
    lost = [names[i] for i in range(len(nodes)) if i not in seen and names[i]]
    # Los nodos sin nombre que quedaron en un rincón aislado se descartan.
    keep = [i for i in range(len(nodes)) if i in seen or names[i]]
    new = {old: k for k, old in enumerate(keep)}
    nodes = [nodes[i] for i in keep]
    names = [names[i] for i in keep]
    edges = [(new[a], new[b]) for a, b in edges if a in new and b in new]

    return {"lost": lost, "x0": x0, "y0": y0, "w": w, "h": h, "cell": CELL, "solid": solid, "blocked": blocked,
            "nodes": nodes, "names": names, "edges": edges, "seats": seats}
