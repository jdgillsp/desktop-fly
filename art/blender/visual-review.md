# Hognose Blender prototype: independent visual review

Reviewed: 2026-09-07 17:17:56 UTC.

Evidence: direct visual inspection of `renders/hognose-study.png` (v2, 1400 x 1100); comparison with the previously inspected v1 image, now archived as `renders/hognose-study-v1.png`, and the user's hognose photo collage. This is a review of the rendered image, not a verification of topology, rigging, animation, export, or runtime performance.

## Decision

Accept as a Blender visual prototype demonstrating a materially richer presentation. Do not classify it as a finished realistic hognose or a production-ready runtime asset. The image remains visibly stylized, particularly in the head, scale structure, and coiling proportions.

## Observable improvements from v1

- Tan skin and darker blotches now have much clearer contrast; head pigmentation helps connect the head visually to the body.
- The conspicuous white pointed lip pieces no longer read as a row of exposed teeth. The closed mouth is more credible.
- Body-scale relief is less inflated and distracting, although the regular honeycomb pattern remains visible.
- The ground no longer has the conspicuous flowing carved texture. Bark is more irregular and less uniformly corrugated.
- Round eyes, cast shadows, surface variation, and the coherent habitat arrangement support a convincing miniature presentation.

## Remaining gaps, ordered by visual impact

1. **Hognose identity:** The short face still appears broad and flattened, and the characteristic upward shovel-shaped rostral tip is not clearly legible from this camera. The eye and brow region also feels sculpted rather than anatomically resolved. A close-up and side view are needed before considering species identity validated.
2. **Body proportions and pose:** Thick, smooth coils maintain nearly constant girth and visually overwhelm the head. The tightly nested loops feel deliberately arranged; the visible taper does not yet clearly explain the whole animal's anatomy.
3. **Skin and pigmentation:** The repeated rounded scale grid, broad smooth highlights, and simple near-black blotches still read as a textured model. Naturalistic directional overlapping scales, more nuanced blotch boundaries, and irregular pigmentation would help. The head's long incised polygon boundaries remain conspicuous.
4. **Habitat materials:** The substrate still reads as a smooth solid display base with scattered grains, rather than a bed of loose sand. Bark has chunky repeated relief, while leaves remain thin, similar cutouts. These props support composition but not close-up realism.
5. **Contact:** The plant appears raised on a dark gap beneath its lowest leaves rather than clearly rooted. Some leaf tips lift naturally, but their thin flat shapes and shadows can still suggest hovering. Contact should be checked from additional angles.

## Scope of confidence

The targeted revision is visibly better than v1 and sufficient for the user's request to try Blender. It demonstrates a useful asset-authoring direction, not completion of the larger creature-and-habitat upgrade. No animation, glTF integration, desktop-size readability, or performance acceptance is implied by this review.


## v3 integration review

The subsequent v3 removes raised brow/crown wires and reduces the eyes, refines
the rostral wedge, and exports a 13,720-vertex skin. DesktopFly now renders that
asset with albedo/height maps, bound to the behavioral spine. 152 shell tests
pass, including grounding, motion, roll, hood and UV validation. Release
snapshots succeeded for all nine species and hognose glass mode.

The earlier realism critique remains relevant: blotches and scales are regular,
and the Blender setting is a stylized miniature. Runtime lighting is simpler
than Cycles; the Blender habitat has not replaced the interactive enclosure.
