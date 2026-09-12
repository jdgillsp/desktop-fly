# Naturalistic miniature direction

The user approved improving species recognition, surface detail, and habitats on
2026-09-07, and requested a further pass after reviewing the Blender hognose.
The target remains recognizable miniature animals with believable materials and
preserved behavioral animation. Glass/connectome presentation remains supported.
This is an integrated preview, not a claim of photorealism.

## Runtime surface inventory

| Surface | Consumer/source | Treatment |
| --- | --- | --- |
| Fruit fly | flybody.rs / FlyRuntime | Smooth eyes, red compound-eye pigment, fine curved bristles, dorsal bands, wing venation |
| Jumping spider | spiderbody.rs / SpiderRuntime | Tapered limbs, dense setae, irregular abdominal spots, seated glints |
| Orb, house, grass spiders | weaverbody.rs / WeaverRuntime | Species markings, cuticle mottling, curved setae, smoother eyes and limbs |
| Koi | koibody.rs / KoiRuntime | Smooth body, subdued overlapping scales, irregular Kohaku pigment, curved translucent fins, seated eyes |
| C. elegans | wormbody.rs / WormRuntime | Continuous spline cuticle and internal lumen, pharyngeal bulb |
| Hognose | hognoseasset.rs / HognoseRuntime | Authored Blender skin, full-resolution albedo and scale relief, live spine deformation |
| Fictional sandworm | sandwormbody.rs / SandwormRuntime | Finer armour annulations, pointed tail, three-dimensional teeth |
| Fly cage | habitatmesh/flycage.rs | Towel fibres, food and cage fasteners |
| Vivarium | habitatmesh/vivarium.rs | Coir, cork fissures, curved foliage and litter |
| Agar plate | habitatmesh/plate.rs | Gel finish and colonies |
| Pond | habitatmesh/pond.rs | Rounded cobbles, wet surfaces and lily veins |
| Terrarium | habitatmesh/terrarium.rs | Varied grit, rounded stones and bark finish |

All surfaces except the literal hognose remain code-native procedural geometry.
The hognose glass mode also remains procedural. Habitat builders preserve alpha,
draw ordering and prop positions used by live interactions. The Blender setting
is a separate study and has not replaced the interactive runtime enclosure.

## Authored hognose

Editable source: art/blender/hognose-study.blend and build_study.py,
hognose_geometry.py, export_hognose.py. Runtime: assets/hognose/mesh.json,
albedo.png and scale_height.png. Original local Blender/Python work; no paid
service, downloaded mesh pack or image generation model was used.

The current export has 21,116 vertices and 39,932 triangles, with 64 radial
sectors and 3072 x 1536 maps. Snout lift, irregular pigment and fine scale relief
were refined. Blender transforms update before export so facial details bind
consistently. The mesh follows the existing behavioral spine, including hooding,
lift, burial, puff, roll and peek. The underside determines grounding height.

## Shared rendering and inspection

Vertex.material carries roughness, specular strength, diffuse wrap and rim.
Vertex.texcoord carries UV, albedo weight and relief amplitude. Desktop and
snapshot share bindings, sRGB albedo decoding, linear-light mip generation and
anisotropic sampling. Height maps use non-color data. Normal derivatives filter
sharp specular highlights. Scaled meshes use inverse-transpose normals.
Desktop buffers grow as detailed geometry exceeds their initial allocation.

Close-up fits actual projected animal geometry and height to 64% of the viewport,
with smooth follow and rapid pullback. Spider bounds exclude appended silk/prey.
The hognose has View > Head close-up with a lower viewing angle. Snapshot
--head-closeup uses the same fit; --size chooses output resolution and --zoom
adds optional magnification. --alt -1 selects a calm hognose diagnostic pose.

## Verification and limits

Prior baseline: 152 shell tests passed and all nine literal snapshots rendered.
Current first inspection pass: 154 tests passed. Subsequent correction pass is
recorded in output/fidelity/inspection/verification.json after final validation.
Image evidence lives in output/fidelity/inspection. Earlier comparison captures
are preserved in output/fidelity/authored and other fidelity subdirectories.

Independent reviewers identified and requested corrections for spider framing,
worm silhouette, koi scale glare and sandworm taper. Their source producers made
a targeted second pass. Authored assets remain stylized; other animals remain
primitive-derived and the habitat still has a miniature aesthetic. No claim is
made that the app matches Cycles lighting, or that performance is profiled on
lower-end hardware.
