# Habitat mode: an optional enclosure

**Status:** v1 implemented and verified on Windows. Branch `habitat-mode`, not
pushed, not merged.

An opt-in rendered container — an aquarium for the koi, a terrarium for the
arthropods and the worm — that bounds the creature to a region of the screen
instead of the whole desktop, with a few props it notices and pushes around.
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

## 2. What was built

| piece | file |
|---|---|
| `Region`, `Habitat`, `Prop`, prop physics, focus | `rust/core/src/habitat.rs` |
| `World.region` / `World.attractor` | `rust/core/src/creature.rs` |
| Curiosity steering (all four bodies) | `body.rs`, `spider.rs`, `worm.rs`, `koi.rs` |
| The tank, drawn | `rust/shell/src/habitatmesh.rs` |
| `Runtime::tick(dt, region, cursor, attractor)`, `Runtime::substrate()` | `rust/shell/src/runtime.rs` |
| Toggle, composition, ledge suppression | `rust/shell/src/main.rs` |
| Menu item, persisted setting | `tray.rs`, `persist.rs` |

### `Substrate` finally does something

`Substrate` has been declared since the creature abstraction landed and nothing
ever branched on it — it was only ever asserted in tests. Habitat mode is its
first real consumer: `Swimmer` gets an aquarium, everything else a terrarium.
The runtime reads it off the **body** (`Body::substrate`), so there is no second
table to drift out of sync with the animals themselves.

### The tank

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
cargo run --release -- --habitat
cargo run --release -- --habitat --creature koi
```

Or tick **Habitat (confine to a tank)** in the tray menu, which persists the
choice. The tank is rebuilt when the creature changes (a koi in a terrarium
would be the wrong enclosure) and moves with the creature across displays,
keeping its props where they were.

There is also `--snapshot out.png --habitat --creature <id>`, which renders the
enclosure offscreen. That is how this was verified without trusting a
screenshot.

## 4. Verification

- 196 tests pass (`cargo test --workspace`); 20 of them are new and specific to
  this feature.
- `every_creature_stays_inside_its_enclosure` runs each of the four creatures
  for a simulated minute in a **small, off-centre** tank with a cursor sweeping
  through it, and asserts none of them ever crosses a wall. Off-centre matters:
  a creature that merely drifts toward (0, 0) would pass a centred test by
  accident.
- `free_roam_still_roams` asserts the fly given the whole display still uses it,
  so the refactor cannot pen the creature in by stealth.
- Both ground-truth suites byte-identical to the parent commit (see §1).
- Live on Windows, 2560×1440: `habitat: aquarium at (876,-436), 760x520`,
  koi placed inside it, 29–30 fps, ledges suppressed.

## 5. Open — for Jesse

1. **Free-floating vs window-anchored.** v1 is free-floating and screen-anchored:
   the tank sits in the lower-right of the display. Anchoring it to a specific
   *window* (so the tank rides your editor) is the more interesting idea and is
   a bigger change — it needs a window to track, a policy for when that window
   is minimised, closed or moved off-screen, and it interacts badly with the
   ledge sense described above. `Region` is the right seam for it either way;
   nothing about the design forecloses it.
2. **Position and size are not user-adjustable.** `default_region` picks 42% ×
   46% of the display, clamped, in the lower-right corner. Dragging it is the
   obvious next ask and is blocked on the click-through constraint — it would
   need a modifier-key grab or a tray-driven "move to corner" cycle rather than
   a drag.
3. **Prop set is fixed and unsaved.** Where the ball ends up is lost on restart.
   Persisting prop positions alongside the habituation state would be a small
   addition to `persist.rs`.
4. **The koi renders small and pale**, in a habitat and out of one — verified
   byte-identical to the pre-habitat build, so this is pre-existing on
   `koi-creature`, not something this branch caused. Worth a look separately;
   see also KOI_PLAN.md §5 on the dorsal fin.
5. **No `--behaviortest` scenarios for habitat mode.** Same reason as the koi's:
   that harness is built around a circuit driving a body, and confinement is a
   property of the body-plus-world. The unit tests cover it; extending the
   harness would cover it in the same place as everything else.
