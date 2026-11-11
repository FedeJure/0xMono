"""Mide en las animaciones dónde quedan la cadera, las manos y el cuerpo,
para dimensionar los muebles del hábitat (altura del asiento, del escritorio,
de la mesa, de la cama).

    python tools/measure_anims.py [clip ...]

Usa el mismo skinning rígido que el motor (tools/preview_anims.py). Unidades:
metros, ejes de Blender (Z arriba, el personaje mira hacia -Y).
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from preview_anims import ROOT, add, mv, pose, sub  # noqa: E402


def measure(mono, clip):
    names = [b["name"].split(":")[-1] for b in mono["bones"]]
    idx = {n: i for i, n in enumerate(names)}
    out = {"hips": [], "lhand": [], "rhand": [], "head": [], "min": [], "max": []}
    for f in clip["frames"]:
        R, J = pose(mono, f)
        out["hips"].append(J[idx["Hips"]])
        out["lhand"].append(J[idx["LeftHand"]])
        out["rhand"].append(J[idx["RightHand"]])
        out["head"].append(J[idx["Head"]])
        pts = []
        for v in mono["verts"]:
            b = v["bone"] or 0
            pts.append(add(mv(R[b], sub(v["co"], mono["bones"][b]["head"])), J[b]))
        out["min"].append([min(p[k] for p in pts) for k in range(3)])
        out["max"].append([max(p[k] for p in pts) for k in range(3)])
    avg = lambda key: [round(sum(p[k] for p in out[key]) / len(out[key]), 3) for k in range(3)]
    lo = [round(min(p[k] for p in out["min"]), 3) for k in range(3)]
    hi = [round(max(p[k] for p in out["max"]), 3) for k in range(3)]
    return {"hips": avg("hips"), "lhand": avg("lhand"), "rhand": avg("rhand"), "head": avg("head"),
            "bbox_min": lo, "bbox_max": hi}


def main():
    mono = json.load(open(os.path.join(ROOT, "assets", "mono.json"), encoding="utf-8"))
    anims = json.load(open(os.path.join(ROOT, "assets", "mono_anims.json"), encoding="utf-8"))
    want = sys.argv[1:]
    for c in anims["clips"]:
        if want and c["name"] not in want:
            continue
        m = measure(mono, c)
        print(c["name"])
        for k, v in m.items():
            print(f"  {k:9s} {v}")


if __name__ == "__main__":
    main()
