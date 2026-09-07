# Creatures #8 and #9: a hognose snake, and a sandworm

**Status:** implemented on Windows. Branch `spider-webs`.

Two more procedural creatures, on the koi's terms (KOI_PLAN.md). Both share
one new way of moving and one new enclosure. Neither has a brain, and neither
pretends to.

---

## 1. Why procedural, again

| creature | is there a connectome? | so |
|---|---|---|
| hognose snake (*Heterodon*) | **No.** No reptile has a synapse-resolution wiring diagram at any scale, and nothing is close. | `Provenance::Procedural`, like the koi |
| sandworm (Shai-Hulud) | **No animal.** It is Frank Herbert's, from *Dune* (1965). | `Provenance::Procedural`, with a `why` that names the book |

The sandworm is the clearest case the set has: the rule against inventing a
connectome to keep the set uniform has no edge cases when there is nothing
that could ever be measured. Its display name says `fictional` as well as
`procedural`, and `the_desert_creatures_are_labelled_procedural` asserts both.

## 2. What is shared

- **`Substrate::Burrower`.** Moves on and *under* loose substrate. Ignores
  window ledges, never flies or jumps, and can be entirely out of sight
  while still being there. The fifth substrate, and the first whose creature
  is sometimes invisible by design.
- **`HabitatKind::SandTerrarium`** (`shell/src/habitatmesh/terrarium.rs`). A
  low glass tank with a ten-unit bed of sand drawn as a block, like the
  plate's agar, so its top surface is something a body can sink beneath and
  be hidden by. A water dish is fixed furniture in a front corner. Props:
  a **cork hide** (new `PropKind::Hide`, an arch the snake goes to and rests
  under; ranked as a target like the spider's bark), cobbles, a succulent, a
  pebble.
- **The path-following spine.** Both bodies use the worm's and the koi's
  trick: the visible spine is resampled from the head's recorded track, so
  a body of fixed length turns without stretching. What they lay across it
  differs.

## 3. The hognose

`rust/core/src/hognose.rs`, `shell/src/hognosebody.rs`, `shell/src/hognosert.rs`.

Three behaviours make it a hognose rather than a rope, and each is pinned by
a test:

1. **The bluff.** A threat on the ground spreads the neck into a hood, stops
   the animal, turns it to face the threat, and produces mock strikes with
   the mouth closed. It does *not* on its own produce a corpse.
2. **Playing dead.** A second fright while bluffing — or a threat that is
   still there when the bluff runs out — flips it belly-up, gapes the mouth,
   hangs the tongue out and goes limp. It is **committed**: further threats
   are ignored, and it does not get up until nothing has bothered it for a
   few seconds. Rolled over, the *ventral* colouring is drawn.
3. **Burrowing.** At night on an idle machine, and now and then on its own,
   it digs in. In the terrarium the sand hides it; in free roam it fades.

Locomotion is lateral undulation: the wave amplitude is nearly uniform along
the body, unlike the koi's tail-weighted one. The tongue flicks in bursts
while it moves.

Senses: a cursor coming close (something large, looming) and clicks (ground
vibration). The threat pulse fires *once per approach*, so the body's
"second fright" is a real second approach. Habituation applies: the drama
fades as the animal learns you.

## 4. The sandworm

`rust/core/src/sandworm.rs`, `shell/src/sandwormbody.rs`, `shell/src/sandwormrt.rs`.

- **It lives under the sand.** Most of the time nothing is drawn but a
  raised, translucent ripple along the spine. Submerged, the body is
  flattened (`SQUASH`) and sunk so that it fits inside the terrarium's sand
  bed without going through the tank floor — `every_creature_fits_between_
  the_floor_and_the_rim_of_its_tank` holds for it.
- **The thumper.** The runtime watches click *times and positions* (content-
  blind, like every sense). Three or more clicks at a regular interval —
  regularity is one minus the coefficient of variation, and a double-click's
  sub-120 ms gap is discarded — are a thumper at the clicks' centre. Typing
  is a weaker rhythm at the cursor. The signal goes out as `pursuit` plus the
  world attractor; the body goes there and **breaches** on arrival: rears the
  front nine segments, opens three jaw lobes on a dark throat rimmed with
  teeth, then settles into a surface cruise and dives.
- **It fears nothing.** No startle; the cursor does nothing. The tray's
  escape test sends it under, because every runtime must answer it.
- **Rings.** The body's radius is modulated ring by ring with the ridges
  travelling at the peristaltic phase, which runs only while it moves.

Habituation is kept but never driven: there is nothing for a sandworm to
learn about you, and the mood line says so honestly rather than being absent.

## 5. Verification

- `cargo test --workspace` (core: 164, shell: 128 at the time of writing).
- The fly's `--simtest --behaviortest` output is byte-identical to the
  parent commit's — neither `lif.rs` nor `roles.rs` is touched.
- `--snapshot` for both, with and without `--habitat`.

## 6. Open

- **No `--behaviortest` scenarios**, for the same reason as the koi: that
  suite is built around a circuit driving a body. The unit tests cover the
  bodies and the runtimes end to end.
- **The hiss is silent.** The app makes no sound anywhere; the bluff is
  visual only.
- **Scale.** A sandworm the length of a fruit fly's flight path is the
  joke, and the terrarium leans into it. A "desert" enclosure sized to the
  screen would be the alternative.
