# Habitat mode: an optional enclosure

**Status:** v3 (one enclosure per animal, section 6) implemented and
verified on Windows, on `spider-webs`. v2 (isometric) is merged.

An opt-in rendered container — a mesh cage for the fly, a tall vivarium for
the spider, an agar plate for the worm, a stone pond for the koi — that bounds
the creature to a region of the screen instead of the whole desktop, with a
few props it notices and pushes around.
Off by default: free roam is untouched, and an existing install behaves exactly
as it did until someone ticks the box.

---

## 1. The one structural change

Every body already turned away from the edge of the world and clamped itself
inside it. Every one of them did it like this:

```rust
let hw = bounds.0 / 2.0 - EDGE_MARGIN;
if self.pos.x.abs() > hw { /* turn toward (0, 0) */ }
```

`bounds` was a **size**, and `abs()` against half of it silently assumed the
world was centred on the scene origin. It always was, so the assumption cost
nothing and was invisible. A tank parked in the corner of the screen is the
first thing that breaks it.

So `World::bounds: (f32, f32)` became `World::region: Region`, a centre plus a
size, and the four bodies ask it questions (`outside`, `clamp_inside`,
`bearing_home`) instead of doing the arithmetic themselves. Free roam is
`Region::centered(display)`.

**This had to be provably behaviour-preserving**, because it touches the fly
and the spider, whose numbers are held against the Swift oracle. Three things
check that:

- `a_centred_region_reproduces_the_old_bounds_arithmetic` asserts, point by
  point, that `Region::centered` gives *the same f32 results* as the old
  expressions. (It found the one genuine disagreement: at the exact centre,
  `(-0.0).atan2(-0.0)` is −π while the new form is 0.0. Unreachable — a body
  would have to be at the centre *and* within a margin of a wall — and
  documented in the test.)
- Both ground-truth suites were diffed against a worktree at the parent commit:
  `--simtest --behaviortest` for the fly and for the spider, **byte-identical**.
- `--snapshot --creature koi` is byte-identical to the pre-change binary.

## 1a. The camera, and why it is habitat-only

Jesse's call after seeing v1: the top-down plate should be an isometric tank —
rectangular, movable, and better looking.

Tilting the camera has one consequence that decides everything else. The app has
always looked straight down, and that is **not** an aesthetic choice: it is what
lets the fly stand on the top edge of *your* browser window. `ScreenSpace` maps a
real window's top edge to a scene y, the fly walks to that y, and it lands on the
real pixels of the real window because the projection is identity in x and y.
Tilt the camera and the fly walks along a line that sits on nothing.

Inside an enclosure none of that applies — the tank is not registered to anything
on the desktop, and window ledges are already suppressed there. So the tilt is
**habitat-only**, and `camera.rs` is the seam:

| | free roam | habitat |
|---|---|---|
| camera | `TopDown` | `Tilted { pitch: 0.90, yaw: 0.60 }` (~52 deg tilt, ~34 deg turn) |
| ground to screen | identity | a 2x2 rotate-and-squash |
| height to screen | invisible | rises up the screen |
| eye distance | 300 (unchanged) | 1500 |
| far plane | 600 (unchanged) | 4000 |

### Yaw is not decoration

The first attempt tilted without yawing, on the reasoning that a screen-aligned
tank keeps "right on screen" meaning "right in the tank". It looked wrong, and
the reason is structural: **a wall at constant x has no extent in screen x when
the yaw is zero**, so the side panes project to bare lines. The result was a
backdrop with a floor, not a container. Yaw is what turns one side wall toward
the viewer and makes the box a box.

The cost is real and worth stating: the ground's axes are no longer the
screen's, so every ground/screen conversion goes through the matrix rather than
a scalar, and the cursor correction is a rotation rather than a stretch.

`Camera::TopDown` reproduces the original matrices exactly — same view matrix,
same view direction, same far plane — and a test asserts each of those, so free
roam is untouched by the whole feature. Pointing free roam at a tilted camera
later is a one-line change plus a decision about what to do with ledges.

The projection stays **orthographic**. Perspective would make the creature's
apparent size depend on where it stood, and constant apparent size across
displays is a decision the port already made (PORT_PLAN.md section 8).

### Three things the tilt required

- **The view direction is no longer +z.** The shader had it hard-coded, which is
  right only looking straight down; specular and rim light were wrong the moment
  the camera moved. It is a uniform now.
- **The cursor is in screen space, not ground space.** Un-projecting it means
  inverting the 2x2 ground-to-screen block. Without it the creature flees a
  pointer that is nowhere near where the user sees it.
- **Placement is a screen question, not a ground one**, and it moved out of
  `core` because of that: under a tilted, yawed camera the ground rectangle and
  its screen footprint are different shapes, and the tank's *walls* occupy
  screen height the ground rectangle knows nothing about. `Camera` now owns
  `screen_offsets`, `place_lower_right`, `clamp_on_screen` and `fit_size`, all
  built from projecting the eight corners of the box — there is no scalar that
  stands in for the answer once yaw is involved.

The camera's `view_dir` is read straight off the view matrix (its third row)
rather than re-derived from the angles. The rotation part is orthonormal, so its
inverse is its transpose, and that is one fewer place for a sign to be wrong.

### What the tilt bought

The koi's `depth` (0 deep, 1 surfaced) has always existed but could only be
expressed as *apparent size*, because a straight-down camera cannot show a fish
rising. In a tank it is a real height in the water column, via
`Runtime::vertical_hint`. The fly's flight altitude needed nothing at all — it
was already real z in the geometry, and simply becomes visible.

## 2. What was built

| piece | file |
|---|---|
| `Region`, `Habitat`, `Prop`, prop physics, focus, placement | `rust/core/src/habitat.rs` |
| The camera seam and the ground/screen mapping | `rust/shell/src/camera.rs` |
| `World.region` / `World.attractor` | `rust/core/src/creature.rs` |
| Curiosity steering (all four bodies) | `body.rs`, `spider.rs`, `worm.rs`, `koi.rs` |
| The enclosures, drawn (one file each since v3) | `rust/shell/src/habitatmesh/` |
| `Runtime::tick(dt, region, cursor, attractor)`, `substrate()`, `vertical_hint()` | `rust/shell/src/runtime.rs` |
| `Frame`, the camera-aware draw | `rust/shell/src/render.rs` |
| The grab chord | `rust/platform/src/windows_senses.rs` |
| Toggle, composition, ledge suppression | `rust/shell/src/main.rs` |
| Menu item, persisted setting | `tray.rs`, `persist.rs` |

### `Substrate` finally does something

`Substrate` has been declared since the creature abstraction landed and nothing
ever branched on it — it was only ever asserted in tests. Habitat mode is its
first real consumer: in v2, `Swimmer` got an aquarium and everything else a
terrarium; since v3 each of the four substrates gets its own enclosure
(section 6). The runtime reads it off the **body** (`Body::substrate`), so
there is no second table to drift out of sync with the animals themselves.

### The tank (v1–v2; superseded by section 6)

Drawn from directly above, because that is the only camera this app has. An
aquarium is therefore not a box in perspective: it is a water plane, a gravel
bed along the near edge with scattered grit, and a bright rim where the glass
catches the light. It reads as a container at a glance and costs a few hundred
triangles. The terrarium is the same construction in soil colours.

Everything is translucent. This is an overlay on someone's desktop, and an
opaque rectangle parked over their work would be intolerable however good it
looked — `nothing_is_fully_opaque` pins that.

Two things about it were wrong on the first attempt and are now pinned by tests:

- **Depth.** A creature's *nominal* plane is z = 0, but its geometry is not:
  the fly's legs and wings reach ~4.7 units below it. The tank was initially
  drawn at z = −3 and the fly stood knee-deep in its own gravel.
  `the_enclosure_is_drawn_behind_every_creature` now *measures* all four
  creatures' true floors and asserts the enclosure clears the lowest.
- **Shadows.** The enclosure and the creature share one pipeline and one vertex
  format, so they go to the GPU as a single mesh — no third buffer, no second
  draw. But the shadow pass draws the same buffer through `vs_shadow`, which
  would have flattened the *tank* onto the shadow plane and painted a large
  black rectangle over the desktop. `Renderer::render` now takes the index at
  which the creature's geometry starts, and shadows only from there.

### Props, and why none of them are clickable

The overlay is click-through by contract: `WS_EX_TRANSPARENT`,
`set_cursor_hittest(false)`, and a README that promises it never intercepts your
mouse. A prop you could click would be a hole in the desktop, and once there is
one hole there is a reason to add another. So interaction runs the other way
round:

- **The creature acts on the props.** It pushes the ball, which rolls, bounces
  off the glass and coasts to a stop — so a session leaves the ball wherever
  the creature put it. It eats the flake, which is then gone and comes back
  somewhere else.
- **The cursor acts on them by proximity**, through the same shadow-of-a-hand
  channel the creatures already sense with. Passing over the tank stirs the
  plants and nudges the ball.

Four props per tank, deliberately: at this scale a tank the size of a browser
window is already busy, and the creature is the thing you are meant to be
watching. Aquarium gets two plants, a pebble and a food flake; terrarium gets
two pebbles, a plant and a ball.

### Noticing

The habitat picks one prop as the current focus and holds it for 4–9 seconds —
re-picking every frame made the creature jitter between props instead of
looking like it had decided. Its position reaches the bodies as
`World::attractor`, a bare `Vec2`. **The bodies never learn what a prop is**,
which is what keeps prop logic out of four separate animals.

Curiosity is one shared constant and is deliberately *weaker* than the
turn-away-from-the-wall reflex: a pet wedged in a corner because it is
transfixed by a ball is worse than one that wanders past it. The koi is the one
special case — it only steers toward food while cruising or hovering, because a
fish that kept correcting toward a flake mid-dart would turn its C-start escape
into a lazy arc, and that escape is not negotiable.

### Window ledges stop existing

A fly loose on the desktop treats window top edges as terrain and latches onto
them. Inside a tank that would mean standing on something that is not in the
enclosure, so `EnvSnapshot::ledges` is cleared before the senses see it when a
habitat is on. Everything else — looms, clicks, the cursor — still applies,
because someone leaning over the glass is exactly what those channels are for.
Confirmed live: `ledges 8` in free roam, `ledges 0` in a terrarium.

## 3. Using it

```
target\release\desktopfly.exe --habitat
target\release\desktopfly.exe --habitat --creature koi
```

Or tick **Habitat (confine to a tank)** in the tray menu, which persists the
choice.

**To move the tank, hold Ctrl+Shift** — it follows the pointer until you let go,
and its rim lights up while you hold it. Deliberately not a click-and-drag: the
overlay never takes a click, and a held chord gets the same grab-and-move feel
without putting a hole in the desktop. The modifiers are read with the same
`GetAsyncKeyState` poll the mouse buttons already use, and modifier keys carry no
typed content, so this stays inside the content-blind rule the other senses
follow. The tank is clamped in *screen* terms, so its walls cannot climb off the
top of the display. The tank is rebuilt when the creature changes (a koi in a terrarium
would be the wrong enclosure) and moves with the creature across displays,
keeping its props where they were.

There is also `--snapshot out.png --habitat --creature <id>`, which renders the
enclosure offscreen. That is how this was verified without trusting a
screenshot.

## 4. Verification

- **201 tests pass** (`cargo test --workspace --exclude dfplatform`); ~30 of them
  are new and specific to this feature.
- Both ground-truth suites **byte-identical** to the parent of the habitat work:
  `--simtest --behaviortest` for the fly and for the spider.
- `--snapshot --creature koi` **byte-identical** to the pre-habitat binary, so
  free roam renders exactly as it did.
- `every_creature_stays_inside_its_enclosure` runs each of the four creatures
  for a simulated minute in a small, **off-centre** tank with a cursor sweeping
  through it. Off-centre matters: a creature that merely drifts toward (0, 0)
  would pass a centred test by accident.
- `free_roam_still_roams` asserts the fly given the whole display still uses it,
  so the refactor cannot pen the creature in by stealth.
- Live on Windows, 2560x1440: `habitat: aquarium at (280,-1028), 720x520`, koi
  cruising inside it at depth 0.31, 29 fps.
- Offscreen renders of both enclosures through the real camera and the real mesh
  builders (`--snapshot --habitat`), which is how the look was judged rather
  than by eyeballing the live overlay.

### Two notes on how this was checked

The `dfplatform` named-pipe test fails while a copy of DesktopFly is running:
the app owns `\\.\pipe\desktopfly-notify`, so the test's `send()` reaches the
app instead of the test's own server. Environmental, and unrelated to this work
— the only platform change here is eleven lines in `windows_senses.rs`.

A running copy also **holds `target/release/desktopfly.exe`**, so `cargo build`
fails to replace it and silently leaves a stale binary behind. That produced two
rounds of "the change had no effect" before it was spotted. Builds during this
work went to `target-iso/` to sidestep it.

## 5. Open — for Jesse

1. **Free roam is still top-down**, deliberately: tilting it would break the
   fly standing on your real window edges. `Camera` is the seam if that trade
   ever looks worth making — it is a one-line change plus a decision about what
   ledges should mean.
2. **Pitch and yaw are constants** (`DEFAULT_PITCH`, `DEFAULT_YAW` in
   `camera.rs`). Easy to tune, not exposed in the UI.
3. **The tank cannot be resized**, only moved. `fit_size` already picks the
   largest that fits the display, so a size control is a small addition.
4. **Prop positions are not saved.** Where the ball ends up is lost on restart;
   persisting them alongside the habituation state is a small change to
   `persist.rs`.
5. **The koi renders small and pale**, in a habitat and out of one — verified
   byte-identical to the pre-habitat build, so this is pre-existing on
   `koi-creature` and not caused by this work. It is more noticeable in a tank,
   where there is scenery to compare it against. Worth a look separately; see
   also KOI_PLAN.md section 5 on the dorsal fin.
6. **No caustics, no refraction, no per-material shader.** The glass is a baked
   fresnel on a single lit pipeline. Adding a second pipeline for water would
   buy a lot visually and is the obvious next step if this is worth more effort.
7. **`snapshot.rs` trips clippy's argument-count lint** (9/7). It was already
   over at 8 before this work; the habitat flag made it worse rather than
   causing it. `render.rs` got the same treatment done properly, via `Frame`.
8. **No `--behaviortest` scenarios for habitat mode.** Same reason as the koi's:
   that harness is built around a circuit driving a body, and confinement is a
   property of the body-plus-world. The unit tests cover it.

## 6. v3: one enclosure per animal

Jesse's note after v2: the tank could be much higher fidelity *for each
animal*. v2 had two containers — a blue box and a brown box — and they were the
same box. v3 replaces the aquarium/terrarium split with **the container each
animal is actually kept in**, chosen from its `Substrate` exactly as before:

| creature | `Substrate` | `HabitatKind` | what it is |
|---|---|---|---|
| *Drosophila* | `WalkerFlier` | `FlyCage` | a framed **mesh rearing cage**: aluminium frame, mesh panels with the weave picked out, a paper floor, a dish of cornmeal medium, a banana and an apple wedge, a spent culture vial with its cotton plug |
| salticid | `WalkerJumper` | `Vivarium` | a **tall arboreal vivarium**, higher than it is wide: acrylic with cross-ventilation rows (low front, high back), coco-fibre floor with leaf litter, a slab of cork bark leaning on the back wall, a twig climbing to a top corner with leaves, and a **silk hammock** spun into the top back corner |
| *C. elegans* | `Crawler` | `AgarPlate` | a **square petri dish** a tenth the height of the others: a block of amber agar, a milky *E. coli* lawn with a ragged edge and grazed patches, sinuous worm tracks in the surface, and a strip of lab tape on the front with marker strokes |
| koi | `Swimmer` | `Pond` | a **raised stone pond**: a ring of coping stones of uneven length and height, a dark liner, river cobbles piled toward the edges, lily pads on the surface (one flowering), and floating pellets |

### What changed in `core`

- `HabitatKind` has four variants and `HabitatKind::ALL`. `is_wet()` is the
  one thing the physics branches on: food floats and the creature has a water
  column only in the pond.
- `PropKind` grew `Dish`, `Fruit`, `Vial`, `Bark`, `Twig`, `Lawn`, `LilyPad`,
  `Cobble`, and lost `Ball` (nothing stocked it any more). `Prop` carries a
  `variant` so the two fruit and the three pads are not identical.
- Standing food (`Dish`, `Fruit`, `Lawn`) is a target that is never used up —
  the fly feeds on the fruit all day, the worm grazes the lawn. Pond pellets
  are still eaten and replaced. The spider's enclosure has no food in it, so
  its bark and twig are **perches**, a lower-ranked target; it patrols them.
  Among equal-ranked targets the pick is random, so the fly visits all three.
- The lily pad took over the ball's push-and-settle physics with pond
  constants (weaker shove, heavier drag, dead bounce off the stones) and a
  slow drift of its own.
- Stocking is per kind and per prop: bark against the back wall, the vial
  along a side wall, the lawn near the centre of the plate, and the lawn's
  radius scales with the plate.

### What changed in the shell

`habitatmesh.rs` became a module: `habitatmesh/mod.rs` (heights, lifts,
dispatch, the walls, tests), `prims.rs` (slab, tube, disc, blob, ribbon,
frond, tuft), and one file per enclosure. Three things are now **per kind**:

- **Wall height** (`wall_height`): 170 for the cage, 230 for the vivarium, 18
  for the dish, 92 for the pond. This is the largest single difference on
  screen and it is a fact about the animal, not a style choice.
- **Footprint** (`footprint`): the vivarium is narrow for its height, the pond
  a little wider than a tank.
- **Lift and shadow plane** (`creature_lift`, `shadow_z`): the worm stands on
  the *agar*, not the bottom of the dish, and its shadow falls on the gel.
  `every_creature_fits_between_the_floor_and_the_rim_of_its_tank` measures
  against the per-kind rim and, for the worm, the agar surface.

### The bug the tall box exposed

The camera is yawed, so it looks in over one **side** wall as well as the
front. In v2 that side wall was drawn in the back pass with the other two,
writing depth before anything inside the tank. Everything in its screen
footprint — a floor point within roughly 120 units of that wall in the cage,
160 in the vivarium, and anything with height — then *failed the depth test*
and vanished: the vivarium's twig, its plant and its pebble were all in the
mesh and none of them were on screen. The creature walking past that wall
would have vanished the same way.

`Ctx::far_walls` and `Ctx::near_side` split the walls by the sign of the view
direction, and the near side goes into the front mesh with the front pane. The
cage's weave and frame bars and the vivarium's vent rows follow the same
split. `the_walls_the_camera_looks_over_are_drawn_in_front` pins it: for every
walled kind, a triangle spanning floor to top and near edge to far edge at the
near side's x is in the front mesh and not the back, and vice versa for the far
side.

Two smaller things pinned while at it: `the_vivarium_furnishes_its_height`
asserts the bark and the twig reach more than half-way up the box — an
arboreal tank with furniture on the floor is a terrarium — and
`lily_pads_float_on_the_surface_in_front_of_the_fish` asserts a pad is in the
front mesh at surface height and not the back, since the koi rises to just
under it.

### Verification

- `cargo test -p dfcore`: 109 pass. `cargo test -p dfshell`: 90 pass.
- `dfcore --simtest --behaviortest` and `dfcore --creature salticid --simtest
  --behaviortest`: all pass; neither the LIF nor any body was touched.
- `--snapshot --habitat --creature <id>` for all four, inspected: the cage,
  dish and pond read at a glance; the vivarium's furniture was the round that
  found the wall-order bug.
- The `dfplatform` pipe test fails for the reason in section 4, with a copy
  of the app running.

### Still open

- **The silk retreat is faint.** It is a translucent white sheet against
  whatever wallpaper is behind the tank; on a light desktop it will all but
  disappear. A second pass could give it thread-by-thread structure.
- **No lid, no water caustics, no refraction** — section 5 item 6 still
  stands, and the pond would benefit most.
- **The spider does not use the walls.** The vivarium is tall and the animal
  patrols the floor furniture; letting it climb the bark is body work, not
  enclosure work.

## 7. v4: a tank you can angle, size and stock

Section 5 left three things open, and they turned out to be one feature: the
enclosure was a fixed diorama the user could only slide around. Items 2 (pitch
and yaw are constants), 3 (the tank cannot be resized) and 4 (prop positions are
not saved) are now closed.

### The decisions

**The view the user owns is not the camera.** `HabitatView { pitch, yaw, zoom }`
sits in front of `Camera` rather than inside it, because zoom is not a camera
property at all. The projection is orthographic on purpose — constant apparent
size across displays (PORT_PLAN.md §8) — so "bigger" cannot be a dolly. It is
the *tank* that is built bigger, and the creature keeps the size it always had,
standing in a larger room. A perspective dolly would have shrunk the animal
along with its enclosure, which is precisely the thing the port decided against.

**Clamping lives in one place.** `HabitatView::camera()` clamps on the way out,
so no caller can produce a degenerate projection however it got its numbers — a
hand-edited settings file included. The ends are chosen, not arbitrary: pitch
stops short of straight-down (where the walls project to lines and the box stops
being a box — see `Camera::Tilted`) and short of edge-on (where the floor
collapses to a sliver). Zoom is applied *before* `fit_size`, so the top of its
range means "as large as this display allows" rather than "off the edge".

**Three chords, all modifier-only.** The overlay is click-through by contract,
so there is no widget to drag and no scroll wheel to read: the vocabulary
available is modifier keys, which say nothing about what is being typed. Each
control is a pair of them and the pointer supplies the magnitude:

| chord | does | reads |
|---|---|---|
| Ctrl+Shift | move the tank | the pointer's **position** — the tank ends up under it |
| Ctrl+Alt | turn and tilt | the pointer's **motion** since the last frame |
| Shift+Alt | grow and shrink | the pointer's **vertical** motion |

Exactly-matching pairs, so holding all three is not silently one of the three.
Move reads an absolute position because there is a place the tank should end up;
turn and size read a delta because no absolute position could mean an angle.

**The delta is screen-space, and that is not a detail.** The orbit chord changes
the camera, and the *ground* position of a stationary pointer changes with it.
Feeding a ground delta back into the camera is a loop that runs away the moment
the user stops moving the mouse. The delta is also only taken when the same
chord was held on the previous frame, so reaching for the keys does not jerk the
view by however far the mouse happened to have travelled.

**A resize carries the props; it no longer restocks them.** `reshape` used to
re-stock on any size change, which was free when the only resize was a display
switch. With a zoom control held down it would reshuffle the furniture on every
frame — and lose the lily pad you had watched the fish nose into a corner. Props
now move proportionally and keep their real size, exactly as the creature does.
The one exception is the bacterial lawn, which is poured to fill its plate:
`PropKind::scales_with_tank` names it, rather than the resize path special-casing
a variant.

**Contents are managed through fixed menu slots.** `PropKind::catalogue` says
what may go in each enclosure — cork bark in a pond is a mistake, not a
decorating choice — and the tray carries four Add/Remove pairs that are
relabelled per enclosure rather than rebuilt. Rebuilding the menu would
invalidate every `MenuId` the tray is holding, and a stale id is a menu item
that does the wrong thing. Per-kind and total caps (`MAX_PROPS`) mean no
combination of additions can bury the animal.

**Removal drops the focus rather than repairing it.** `Habitat::focus` is an
index into `props`; after a removal a stale one points at whatever slid into the
gap. `retarget` picks again on the next frame, so dropping it costs nothing.

**Positions persist as fractions, not coordinates.** The tank is placed against
the display, so its centre and size differ between a laptop panel and a 1440p
monitor and between one zoom setting and another. Absolute positions would
restore a carefully-arranged pond onto the desk beside it. Contents are keyed by
*enclosure kind* rather than by creature: a pond is a pond whichever fish is in
it. `None` means "never arranged" and the tank stocks itself; `Some([])` is an
enclosure the user deliberately emptied, and re-stocking that on every launch
would make emptying it impossible.

**Writes are debounced to chord release**, not per frame, and joined to the
60-second habituation checkpoint and both quit paths.

### Using it

```
desktopfly.exe --habitat
```

Then, with the pointer over the desktop:

- **Ctrl+Shift** — move the tank (unchanged).
- **Ctrl+Alt** — drag left/right to turn it, up/down to tilt the camera.
- **Shift+Alt** — drag up to grow it, down to shrink it.

The rim lights up while any of the three is held. Everything is also in the tray
under **View** (which shows the current numbers, and greys out *Reset View* when
there is nothing to reset) and **Contents** (Add/Remove per prop, and *Restock*).
The angles and the arrangement are remembered.

Offscreen, for judging a change without trusting a screenshot:

```
desktopfly.exe --snapshot out.png --habitat --creature salticid --pitch 0.30 --yaw -0.9 --tank 1.4
```

`--pitch`, `--yaw` and `--tank` render the enclosure at that view. This is how
the controls below were checked.

### Verification

- **223 tests pass** (`cargo test --workspace --exclude dfplatform`), 12 of them
  new: that the default view still reproduces `DEFAULT_PITCH`/`DEFAULT_YAW`
  exactly, that no amount of adjustment yields a degenerate projection, that the
  ground round-trip holds at *every* reachable angle (not just the shipped one),
  that zoom grows the tank and never off the display, that a resize carries
  props instead of restocking them, that only the lawn grows with its tank, that
  the caps hold under abuse, that a removal drops a stale focus, and that an
  arrangement round-trips through disk onto a differently-sized tank.
- Both ground-truth suites **byte-identical** to the parent commit:
  `dfcore --simtest --behaviortest` for the fly and for the salticid.
- Offscreen renders through the real camera and the real mesh builders at
  pitch 0.30 / 0.90 / 1.25 and yaw ±, confirming the floor foreshortens as the
  pitch rises and that a negative yaw shows the *other* side wall.
- Live on Windows, 2560x1440: a seeded settings file with a bad prop name and a
  prop from another enclosure restored the one legitimate prop and dropped the
  rest, and its `pitch 0.42, yaw -0.8, zoom 1.6` produced an 852x666 tank where
  the default view gives 533x416.

### Still open

- **The chords are undiscoverable** unless you read this file or find the tray.
  There is nowhere to put a hint: the overlay cannot draw UI it would then have
  to explain, and a first-run toast is a window this app does not have.
- **Zoom is applied to the tank's footprint, not to its height.** A very large
  tank is proportionally shallower than a small one, because `top_z` is a
  per-kind constant. It reads fine across the current range; it would not if the
  range were widened.
- **`snapshot.rs` still trips clippy's argument-count lint** — §5 item 7 stands,
  and swapping `habitat: bool` for `Option<HabitatView>` neither helped nor hurt.
- **Contents are per enclosure kind, not per creature.** Two creatures sharing a
  substrate share the tank they were given. This is deliberate, but it means a
  second flier would inherit the fly's arrangement.
