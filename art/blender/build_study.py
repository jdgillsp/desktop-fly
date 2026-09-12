"""DesktopFly hognose study: editable Blender source and runtime skin export.

Root direction/integration. Geometry and habitat are independent authored modules.
All meshes and texture maps are created locally; no external images or paid assets.
Execute in Blender through the connected MCP, or with Blender --python this_file.
"""
from pathlib import Path
import importlib.util
import json
import math
import bpy
import numpy as np
from mathutils import Vector

ROOT = Path(__file__).resolve().parent
PREFIX = 'DF_Hognose_Study_v4'


def load_module(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / (name + '.py'))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def image_map(name, pixels, color=True):
    height, width = pixels.shape[:2]
    image = bpy.data.images.new(name, width=width, height=height, alpha=True)
    image.colorspace_settings.name = 'sRGB' if color else 'Non-Color'
    if color:
        pixels = pixels.copy()
        # Generated image buffers are scene-linear. The authored swatches are
        # sRGB, so decode once before Blender encodes the saved PNG.
        rgb = pixels[..., :3]
        pixels[..., :3] = np.where(rgb <= .04045, rgb / 12.92, ((rgb + .055) / 1.055) ** 2.4)
    image.pixels.foreach_set(pixels.astype(np.float32).ravel())
    image.filepath_raw = str(ROOT / 'textures' / (name + '.png'))
    image.file_format = 'PNG'
    image.save()
    image.pack()
    return image


def authored_body_maps():
    width, height = 3072, 1536
    u = (np.arange(width, dtype=np.float32)[None, :] + .5) / width
    v = (np.arange(height, dtype=np.float32)[:, None] + .5) / height
    rng = np.random.default_rng(771)
    phase = u * 28.0
    spot_id = np.floor(phase)
    center = .25 + .009 * np.sin(spot_id * 1.73)
    radius = .31 + .060 * np.sin(spot_id * 2.41) + .023 * np.cos(spot_id * 4.73)
    spot = ((phase % 1 - .5) / radius) ** 2 + ((v - center) / (.082 + .015 * np.sin(spot_id * 1.17))) ** 2
    flank_phase = (phase + .48) % 1 - .5
    flank = np.minimum((flank_phase / .26) ** 2 + ((v - .035) / .047) ** 2,
                       (flank_phase / .26) ** 2 + ((v - .465) / .047) ** 2)
    edge_wobble = .09 * np.sin(u * 790 + v * 140) + .055 * np.sin(u * 1300 + v * 600) * np.sin(v * 960)
    patch = np.minimum(spot, flank) + edge_wobble
    ground = np.array([.64, .565, .445], dtype=np.float32)
    dark = np.array([.29, .24, .18], dtype=np.float32)
    outline = np.array([.775, .698, .555], dtype=np.float32)
    rgb = np.broadcast_to(ground, (height, width, 3)).copy()
    variation = .026 * np.sin(u * 105 + v * 23) + .013 * np.sin(u * 489 - v * 97) * np.sin(v * 373) + .009 * rng.normal(size=(height, width))
    rgb += variation[..., None]
    edge = np.clip((1.20 - patch) / .16, 0, 1)
    rgb = rgb * (1 - edge[..., None]) + outline * edge[..., None]
    inside = np.clip((1.02 - patch) / .10, 0, 1)
    rgb = rgb * (1 - inside[..., None]) + (dark + variation[..., None]) * inside[..., None]
    # Fine dark edging, broken pigment and symmetric head bars distinguish the
    # marking from clean paint dots on a model.
    edging = np.clip(1 - np.abs(patch - .98) / .16, 0, 1) * .24
    rgb *= 1 - edging[..., None]
    head_u = u / .061
    crown = ((head_u - .61) / .28) ** 2 + ((np.abs(v - .25) - .092) / .060) ** 2
    nape = ((head_u - .94) / .16) ** 2 + ((v - .25) / .077) ** 2
    head_mask = np.clip((1.1 - np.minimum(crown, nape)) / .25, 0, 1)
    head_rgb = ground + variation[..., None]
    head_rgb = head_rgb * (1 - head_mask[..., None]) + dark * head_mask[..., None]
    rgb = np.where((u < .059)[..., None], head_rgb, rgb)
    belly = np.clip((np.abs(v - .25) - .225) / .075, 0, 1)
    belly = np.broadcast_to(belly, (height, width))
    belly_rgb = np.empty_like(rgb)
    belly_rgb[:] = [.79, .735, .605]
    ventral_marks = ((np.sin(u * 290) + np.cos(v * 55 + u * 60)) > .55) & (v > .56) & (v < .95)
    belly_rgb[ventral_marks] = [.20, .18, .15]
    rgb = rgb * (1 - belly[..., None]) + belly_rgb * belly[..., None]
    # Overlapping oval scales and a narrow central keel; no baked illumination.
    scale_u = u * 245 + .055 * np.sin(v * 55 + u * 180)
    row = np.floor(scale_u)
    fu = scale_u % 1 - .5
    fv = (v * 36 + .5 * (row % 2) + .045 * np.sin(row * 1.87)) % 1 - .5
    oval = ((np.abs(fu) / .56) ** 1.6 + (np.abs(fv) / .48) ** 1.6) ** .625
    rim = np.clip((oval - .86) / .15, 0, 1)
    dome = np.clip(1 - oval ** 2, 0, 1) * .19
    keel = np.exp(-(fv / .065) ** 2) * np.clip(1 - (fu / .55) ** 2, 0, 1) * .13
    relief = np.clip(.36 + dome + keel - rim * .075, 0, 1)
    relief = relief * (1 - belly * .7) + (.40 + .12 * np.cos(u * 172 * math.tau)) * belly * .7
    rgb *= (1 - rim[..., None] * .075)
    # Irregular pigment per scale; larger, quieter shields on the crown.
    scale_tone = .012 * np.sin(row * 2.19 + np.floor(v * 36) * 3.77)
    rgb += scale_tone[..., None] * (1 - belly[..., None])
    relief = np.where(u < .055, .40 + .035 * np.sin(u * 910) * np.sin(v * 73), relief)
    snout = np.clip((.012 - u) / .008, 0, 1)
    rgb = rgb * (1 - snout[..., None] * .7) + np.array([.75,.67,.52]) * snout[..., None] * .7
    albedo = np.ones((height, width, 4), dtype=np.float32)
    albedo[..., :3] = np.clip(rgb, 0, 1)
    bump = np.ones_like(albedo)
    bump[..., :3] = relief[..., None]
    return image_map('hognose_albedo', albedo), image_map('hognose_scale_height', bump, False)


def material(name, color, roughness=.5):
    mat = bpy.data.materials.new(PREFIX + '_' + name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get('Principled BSDF')
    bsdf.inputs['Base Color'].default_value = (*color, 1)
    bsdf.inputs['Roughness'].default_value = roughness
    mat.diffuse_color = (*color, 1)
    return mat, bsdf


def build_materials():
    albedo, height = authored_body_maps()
    body, bsdf = material('Scales', (.40, .29, .16), .47)
    nodes, links = body.node_tree.nodes, body.node_tree.links
    tex = nodes.new('ShaderNodeTexImage'); tex.image = albedo
    tex.label = 'Authored pigment: dorsal saddles / offset flank blotches'
    tex.location = (-550, 120)
    links.new(tex.outputs['Color'], bsdf.inputs['Base Color'])
    relief = nodes.new('ShaderNodeTexImage'); relief.image = height
    relief.location = (-550, -160)
    bump = nodes.new('ShaderNodeBump')
    bump.inputs['Strength'].default_value = .38
    bump.inputs['Distance'].default_value = .010
    links.new(relief.outputs['Color'], bump.inputs['Height'])
    links.new(bump.outputs['Normal'], bsdf.inputs['Normal'])
    bsdf.inputs['Specular IOR Level'].default_value = .29
    mats = {'body': body}
    for key, color, rough in [
        ('head', (.37, .285, .17), .48),
        ('belly', (.68, .565, .385), .54),
        ('eye', (.006, .004, .002), .12),
        ('iris', (.18, .10, .035), .25),
        ('crease', (.13, .095, .053), .62),
        ('tongue', (.16, .035, .025), .48),
    ]:
        mat, p = material(key, color, rough)
        if key == 'head':
            texture = mat.node_tree.nodes.new('ShaderNodeTexImage')
            texture.image = albedo
            mat.node_tree.links.new(texture.outputs['Color'], p.inputs['Base Color'])
        if key == 'eye':
            p.inputs['Coat Weight'].default_value = .45
            p.inputs['Coat Roughness'].default_value = .08
        if key in ('head', 'belly'):
            n, l = mat.node_tree.nodes, mat.node_tree.links
            noise = n.new('ShaderNodeTexNoise')
            noise.inputs['Scale'].default_value = 72
            noise.inputs['Detail'].default_value = 3
            bump = n.new('ShaderNodeBump')
            bump.inputs['Strength'].default_value = .22
            bump.inputs['Distance'].default_value = .010
            l.new(noise.outputs['Fac'], bump.inputs['Height'])
            l.new(bump.outputs['Normal'], p.inputs['Normal'])
        mats[key] = mat
    return mats


def aim(obj, point):
    obj.rotation_euler = (Vector(point) - obj.location).to_track_quat('-Z', 'Y').to_euler()


def area(collection, name, location, energy, size, color):
    data = bpy.data.lights.new(PREFIX + name, 'AREA')
    data.energy, data.shape, data.size, data.color = energy, 'DISK', size, color
    obj = bpy.data.objects.new(data.name, data)
    collection.objects.link(obj); obj.location = location; aim(obj, (0, 0, 0))
    return obj


def main():
    scene = bpy.data.scenes.new(PREFIX)
    collection = bpy.data.collections.new(PREFIX + '_Asset')
    scene.collection.children.link(collection)
    stage = bpy.data.collections.new(PREFIX + '_Setting')
    scene.collection.children.link(stage)
    studio = bpy.data.collections.new(PREFIX + '_Studio')
    scene.collection.children.link(studio)
    bpy.context.window.scene = scene
    mats = build_materials()
    snake = load_module('hognose_geometry').build_hognose(scene, collection, mats)
    asset_dir = ROOT.parent.parent / 'assets' / 'hognose'
    asset_dir.mkdir(parents=True, exist_ok=True)
    bpy.context.view_layer.update()
    export_info = load_module('export_hognose').export_hognose(snake, collection, str(asset_dir / 'mesh.json'))
    for source, target in [('hognose_albedo.png', 'albedo.png'), ('hognose_scale_height.png', 'scale_height.png')]:
        img = bpy.data.images.load(str(ROOT / 'textures' / source), check_existing=False)
        img.scale(3072, 1536)
        img.filepath_raw = str(asset_dir / target)
        img.save()
    habitat = load_module('hognose_habitat').build_habitat(scene, stage)
    camera_data = bpy.data.cameras.new(PREFIX + '_Camera')
    camera = bpy.data.objects.new(camera_data.name, camera_data)
    studio.objects.link(camera)
    camera.location = (6.4, -10.2, 7.4)
    aim(camera, (0, -.1, .25))
    camera_data.type = 'ORTHO'; camera_data.ortho_scale = 9.8
    camera_data.lens = 65
    scene.camera = camera
    area(studio, '_Key', (-3.5, -4.5, 7.0), 1050, 5.0, (1.0, .92, .80))
    area(studio, '_Fill', (5.0, -1.5, 4.0), 650, 4.0, (.80, .88, 1.0))
    area(studio, '_Rim', (1.0, 5.0, 6.0), 1200, 4.0, (1.0, .87, .65))
    world = bpy.data.worlds.new(PREFIX + '_World'); world.use_nodes = True
    world.node_tree.nodes['Background'].inputs['Color'].default_value = (.32, .36, .42, 1)
    world.node_tree.nodes['Background'].inputs['Strength'].default_value = .28
    scene.world = world
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 80
    scene.cycles.use_denoising = True
    prefs = bpy.context.preferences.addons['cycles'].preferences
    prefs.compute_device_type = 'OPTIX'; prefs.get_devices()
    for device in prefs.devices:
        device.use = device.type == 'OPTIX'
    scene.cycles.device = 'GPU'
    scene.render.resolution_x, scene.render.resolution_y = 1400, 1100
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = 'PNG'
    scene.render.filepath = str(ROOT / 'renders' / 'hognose-study.png')
    scene.view_settings.view_transform = 'AgX'
    scene.render.film_transparent = False
    scene['asset_status'] = 'Authored source with DesktopFly runtime mesh export'
    scene['authors'] = 'root integration/materials; blender_snake geometry; blender_habitat setting'
    scene['license_basis'] = 'Original locally authored geometry and procedural texture maps'
    bpy.context.view_layer.update()
    for area_ui in bpy.context.screen.areas:
        if area_ui.type == 'VIEW_3D':
            area_ui.spaces.active.region_3d.view_perspective = 'CAMERA'
            area_ui.spaces.active.shading.type = 'MATERIAL'
    bpy.ops.wm.save_as_mainfile(filepath=str(ROOT / 'hognose-study.blend'), check_existing=False)
    return {'export': export_info, 'scene': scene.name, 'objects': len(scene.objects),
            'snake_objects': len(snake['objects']), 'habitat_objects': len(habitat['objects']),
            'blend': str(ROOT / 'hognose-study.blend'), 'render': scene.render.filepath}


if __name__ == '__main__':
    result = main()
