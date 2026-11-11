"""Exporta un escenario (el hábitat) de Blender a JSON para `tools/build_assets.py`.

    blender -b assets/house.blend --python tools/blender/export_scene.py -- assets/house.json

Exporta la colección `Habitat`:
- cada objeto de malla con su tipo (`kind`: static / prop / state:<nombre>),
  triangulado, con el color plano de cada cara (atributo `Col`), la normal
  de la cara y, si su material tiene textura, la UV de cada esquina;
- los empties `spot_*` y `wp_*` (posición y hacia dónde miran);
- las cámaras `cam_*` (posición, orientación y lente).
"""

import json
import os
import sys
from collections import Counter

import bpy


def material_info(mat):
    info = {"name": mat.name, "image": None, "uv": None, "extension": None}
    if mat and mat.use_nodes:
        nodes = mat.node_tree.nodes
        tex = next((n for n in nodes if n.type == "TEX_IMAGE" and n.image), None)
        uvn = next((n for n in nodes if n.type == "UVMAP"), None)
        if tex:
            info["image"] = os.path.normpath(bpy.path.abspath(tex.image.filepath, library=tex.image.library))
            info["extension"] = tex.extension
            info["uv"] = uvn.uv_map if uvn else None
    return info


def export_mesh(ob):
    me = ob.data
    mw = ob.matrix_world
    nm = mw.to_3x3().inverted().transposed()
    me.calc_loop_triangles()
    attr = me.color_attributes.get("Col")
    mats = [material_info(m) for m in me.materials]
    uv_layers = {u.name: u.uv for u in me.uv_layers}

    def color(poly):
        if attr is None:
            return [255, 255, 255]
        c = Counter(tuple(round(x * 255) for x in attr.data[li].color_srgb[:3]) for li in poly.loop_indices)
        return list(c.most_common(1)[0][0])

    tris = []
    for lt in me.loop_triangles:
        poly = me.polygons[lt.polygon_index]
        t = {"v": list(lt.vertices), "color": color(poly), "mat": poly.material_index if mats else -1,
             "n": [round(c, 4) for c in (nm @ poly.normal).normalized()]}
        if mats and mats[poly.material_index].get("uv") in uv_layers:
            uv = uv_layers[mats[poly.material_index]["uv"]]
            t["uv"] = [[round(uv[li].vector[0], 5), round(uv[li].vector[1], 5)] for li in lt.loops]
        tris.append(t)
    return {
        "name": ob.name,
        "kind": ob.get("kind", "static"),
        "verts": [[round(c, 5) for c in (mw @ v.co)] for v in me.vertices],
        "tris": tris,
        "materials": mats,
    }


def relative_images(materials, out):
    """Las imágenes, relativas a la carpeta del JSON (con `/`): así el JSON no
    guarda la carpeta del usuario y `build_assets.py` las encuentra igual."""
    base = os.path.dirname(os.path.abspath(out))
    for m in materials:
        if m.get("image"):
            try:
                m["image"] = os.path.relpath(m["image"], base).replace("\\", "/")
            except ValueError:  # otra unidad en Windows
                m["image"] = os.path.basename(m["image"])


def main():
    out = sys.argv[sys.argv.index("--") + 1]
    coll = bpy.data.collections.get("Habitat")
    objs = coll.all_objects if coll else bpy.context.scene.objects
    # Solo el nombre del .blend: la ruta completa delata la carpeta del usuario.
    data = {"source": bpy.path.basename(bpy.data.filepath), "objects": [], "spots": [], "waypoints": [], "cameras": []}
    for ob in sorted(objs, key=lambda o: o.name):
        if ob.type == "MESH":
            data["objects"].append(export_mesh(ob))
        elif ob.type == "EMPTY" and ob.name.startswith(("spot_", "wp_")):
            entry = {"name": ob.name, "pos": [round(c, 5) for c in ob.matrix_world.translation],
                     "yaw": round(ob.matrix_world.to_euler().z * 57.29578, 2)}
            data["spots" if ob.name.startswith("spot_") else "waypoints"].append(entry)
        elif ob.type == "CAMERA":
            q = ob.matrix_world.to_quaternion()
            cam = ob.data
            data["cameras"].append({
                "name": ob.name,
                "pos": [round(c, 5) for c in ob.matrix_world.translation],
                "rot": [round(c, 6) for c in q],
                "lens": cam.lens,
                "sensor": cam.sensor_width,
            })
    for o in data["objects"]:
        relative_images(o["materials"], out)
    with open(out, "w", encoding="utf-8") as f:
        json.dump(data, f, separators=(",", ":"))
    tris = sum(len(o["tris"]) for o in data["objects"])
    print(f"escenario: {len(data['objects'])} objetos, {tris} triángulos, {len(data['spots'])} lugares, "
          f"{len(data['waypoints'])} puntos de paso, {len(data['cameras'])} cámaras -> {out}")


main()
