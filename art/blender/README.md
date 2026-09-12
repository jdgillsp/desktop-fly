# Hognose Blender study

An editable visual prototype for the next DesktopFly fidelity tier, created in
the user's running Blender 5.1.1 session on 2026-09-07. The original default scene
is preserved. `DF_Hognose_Study_v4` is the revised study scene.

- `hognose-study.blend`: packed materials, continuous snake mesh, face details,
  habitat objects, lights and cameras. Select the active v4.004 scene.
- `renders/hognose-study.png`: revised full study, Cycles / RTX 5090.
- `renders/hognose-head-detail.png`: lower-angle face detail.
- `renders/hognose-study-v1.png` and `hognose-study-v1.blend`: first-pass archive.
- `textures/`: authored 3072 Ã— 1536 pigment and scale-height maps, also packed.
- `build_study.py`: repeatable scene assembly, materials, lighting and saving.
- `hognose_geometry.py`: continuous coiled mesh, UVs, eyes, closed labial band,
  nostrils and a continuous ventral lip.
- `hognose_habitat.py`: sandy setting, grains, cork, stones, leaves and succulent.

The maps and meshes are authored locally using Blender/Python; no image-generation
model, downloaded asset pack, or paid service was used for this prototype.
Source authors: root (material/scene integration), blender_snake (anatomy),
blender_habitat (setting). Independent critique: blender_critic.

## Runtime integration

The user approved proceeding from the displayed study on 2026-09-07. The v3
literal hognose is now exported to `assets/hognose/mesh.json` and deformed by
`rust/shell/src/hognoseasset.rs`. The binding follows the existing behavioral
spine; it does not require a Blender armature. Albedo and scale-height maps are
embedded in both desktop and snapshot builds. Glass keeps the procedural mesh.

Run `build_study.py` in Blender to recreate the editable scene and runtime export.
It creates a new scene and preserves existing scenes. The source map resolution
is 3072 × 1536; runtime maps are 3072 × 1536. Mesh budget: 21,116 vertices,
39,932 triangles. All geometry and textures are original local procedural work.

Cycles study images show the authored habitat and studio lighting. Actual app
captures are in `output/fidelity/authored/`; the runtime habitat remains the
interactive procedural enclosure. Other creatures retain their earlier geometry
and material improvements. This first authored asset is still stylized: further
species-specific sculpting and habitat asset work remain useful.
