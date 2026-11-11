"""Exporta un personaje de Blender a JSON para `tools/build_assets.py`.

Se ejecuta dentro de Blender (pestaña Scripting, o por el MCP):

    exec(open(r"<repo>/tools/blender/export_character.py").read(),
         {"OBJECT": "mono", "OUT": r"<repo>/assets/mono.json"})

Exporta la pose de reposo (sin el modificador Armature), triangulada:
- posición y normal suave por vértice, y su hueso dominante;
- por triángulo: material, normal de su cara y color plano (el de la
  mayoría de las esquinas del atributo de color);
- por esquina: UV del mapa que usa el material, si tiene textura;
- materiales con su imagen, mapa de UV y modo de repetición;
- huesos: nombre, padre y cabeza en espacio del mundo.

No convierte nada: la cuantización, las texturas y el formato del motor los
hace `build_assets.py`, que se puede correr sin Blender.
"""

import json
import os
from collections import Counter

import bpy

OBJECT = globals().get("OBJECT", "mono")
OUT = globals().get("OUT")
COLOR_ATTR = globals().get("COLOR_ATTR", "Col")


def material_info(mat):
    info = {"name": mat.name, "image": None, "uv": None, "extension": None}
    if not mat.use_nodes:
        return info
    nodes = mat.node_tree.nodes
    tex = next((n for n in nodes if n.type == "TEX_IMAGE" and n.image), None)
    uvn = next((n for n in nodes if n.type == "UVMAP"), None)
    if tex:
        info["image"] = os.path.normpath(bpy.path.abspath(tex.image.filepath, library=tex.image.library))
        info["extension"] = tex.extension
        info["uv"] = uvn.uv_map if uvn else None
    return info


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


def export():
    ob = bpy.data.objects[OBJECT]
    if ob.mode == "EDIT":
        ob.update_from_editmode()
    me = ob.data
    mw = ob.matrix_world
    nm = mw.to_3x3().inverted().transposed()
    me.calc_loop_triangles()

    arm = ob.parent if ob.parent and ob.parent.type == "ARMATURE" else None
    bones, bone_index = [], {}
    if arm:
        for b in arm.data.bones:
            bone_index[b.name] = len(bones)
            head = arm.matrix_world @ b.head_local
            bones.append({
                "name": b.name,
                "parent": bone_index.get(b.parent.name) if b.parent else None,
                "head": [round(c, 5) for c in head],
            })
    groups = {g.index: g.name for g in ob.vertex_groups}

    verts = []
    for v in me.vertices:
        bone = None
        if v.groups:
            g = max(v.groups, key=lambda g: g.weight)
            bone = bone_index.get(groups.get(g.group))
        n = (nm @ v.normal).normalized()
        verts.append({
            "co": [round(c, 5) for c in (mw @ v.co)],
            "n": [round(c, 4) for c in n],
            "bone": bone,
        })

    attr = me.color_attributes.get(COLOR_ATTR)
    materials = [material_info(m) for m in me.materials]
    uv_layers = {u.name: u.uv for u in me.uv_layers}

    def face_color(poly):
        if attr is None:
            return [255, 255, 255]
        cols = Counter(
            tuple(round(c * 255) for c in attr.data[li].color_srgb[:3]) for li in poly.loop_indices
        )
        return list(cols.most_common(1)[0][0])

    face_colors = [face_color(p) for p in me.polygons]
    tris = []
    for lt in me.loop_triangles:
        poly = me.polygons[lt.polygon_index]
        mat = materials[poly.material_index] if materials else {"uv": None}
        t = {
            "v": list(lt.vertices),
            "mat": poly.material_index,
            "color": face_colors[lt.polygon_index],
            # Normal de la cara original (no del triángulo): los quads no
            # planos se iluminan parejo, como en Blender.
            "n": [round(c, 4) for c in (nm @ poly.normal).normalized()],
        }
        if mat.get("uv") in uv_layers:
            uv = uv_layers[mat["uv"]]
            t["uv"] = [[round(uv[li].vector[0], 5), round(uv[li].vector[1], 5)] for li in lt.loops]
        tris.append(t)

    data = {
        "object": OBJECT,
        # Solo el nombre del .blend: la ruta completa delata la carpeta del usuario.
        "source": bpy.path.basename(bpy.data.filepath),
        "verts": verts,
        "tris": tris,
        "materials": materials,
        "bones": bones,
    }
    out = OUT or os.path.join(os.path.dirname(bpy.data.filepath), OBJECT + ".json")
    relative_images(materials, out)
    with open(out, "w", encoding="utf-8") as f:
        json.dump(data, f, separators=(",", ":"))
    return {"out": out, "verts": len(verts), "tris": len(tris), "bones": len(bones),
            "materials": [m["name"] for m in materials]}


result = export()
