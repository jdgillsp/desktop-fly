# Creature #4: a koi, and the honest way to ship one

**Status:** implemented and verified on Windows. Branch `koi-creature`.

The set so far runs on real wiring: the fly on FlyWire FAFB v783, the spider on
FlyWire modules recombined with one authored connective, the worm on a graded
integrator when its data is present. A koi cannot join them on those terms,
because **the data does not exist**. This records what was built instead, why,
and what would have to change for the koi to become a measured creature.

---

## 1. The problem, stated plainly

There is no synapse-resolution whole-brain connectome for any fish at the scale
FlyWire provides for *Drosophila*. Not for koi, not for goldfish, not for
zebrafish. So there are exactly three options:

| option | verdict |
|---|---|
| Invent a plausible fish connectome | **Never.** The app's entire claim is that the brain data is real. A fabricated dataset that looks like the others is the one failure mode that would poison every other creature by association — PORT_PLAN.md §6.2 rule 4. |
| Run the koi on the fly's circuit and call it a fish | Also no. That is what the spider does, but the spider is *labelled a chimera* and its parts are itemised per neuron. Quietly reusing a fly brain and calling the result a fish would be the same lie with extra steps. |
| Drive it procedurally, and say so | **This.** |

So the koi is `Provenance::Procedural` — a new variant, distinct from
`Authored` (a connectome someone wrote) and `Grown` (one a process generated).
It makes a stronger claim than either: **there are no neurons in the loop at
all.**

## 2. What "labelled" actually means here

Not a footnote. The label is in every place a user or a developer could form a
belief about this creature:

- **Tray, data line:** `PROCEDURAL - no connectome, hand-written behaviour`.
  Deliberately not a dataset name.
- **Tray, creature picker:** `Koi (procedural)`.
- **Console, on startup:** the provenance line, followed by *why* there is no
  connectome and what the future path is.
- **Brain window:** stays closed. `sim()` returns `None` for this creature's
  whole life, so there is nothing to open — rather than opening an empty window
  that implies a brain failed to load.
- **Glass register:** renders an empty glass fish. Every other creature shows
  its connectome glowing inside the body; the koi has none, and showing none is
  the honest rendering.
- **Tests:** `procedural_labels_are_honest` asserts the describe() line contains
  `PROCEDURAL` and `no connectome`, and contains none of `flywire`, `measured`,
  `chimera`, `connectome-derived`. `the_koi_never_claims_to_have_a_brain`
  asserts the runtime exposes no sim, no soma cloud, and a data line that says
  so.

## 3. The zebrafish path — real, and not taken yet

*Danio rerio* is the honest candidate if this creature is ever to run on data,
and it is worth being precise about why it does not qualify today:

- **What exists:** whole-brain *activity* imaging in larvae (light-sheet
  calcium imaging across essentially every neuron), plus EM volumes covering
  parts of the larval brain and detailed reconstructions of specific circuits —
  notably the hindbrain escape network.
- **What does not exist:** a whole-brain synapse-resolution connectome of the
  kind `etl.py` consumes for the fly.

**Verify this before acting on it.** It is a fast-moving field and this note is
a snapshot, not a literature review.

If such a dataset lands, the work is an ETL and a role manifest — *not* a
rewrite. The body, the swimming, the senses and the behaviour states all stay;
only the drive model is replaced. That is exactly what the `Runtime` seam is
for, and the koi is the case that proves the seam holds for a creature with no
simulation at all.

### The Mauthner cell, and not over-claiming it

There is a genuinely elegant piece of symmetry here. A fish's fast escape is
driven by the **Mauthner cell**, a single giant reticulospinal neuron that is
the functional counterpart of the fly's giant fiber: one cell, one decision, a
whole-body escape. The C-start this koi performs is modelled on the behaviour
that cell produces.

It is not simulating a Mauthner cell. It is imitating what one does. If
zebrafish data ever arrives, that is the circuit to wire up first — and at that
point the resemblance stops being a metaphor.

## 4. What was built

| piece | file | note |
|---|---|---|
| Body and locomotion | `rust/core/src/koi.rs` | Carangiform swimming, burst-and-coast, C-start, depth |
| Creature and provenance | `rust/core/src/creature.rs` | `Koi`, `Provenance::Procedural`, `Substrate::Swimmer` |
| Empty manifest | `rust/core/src/roles.rs` | `procedural()` — no populations, because there are none |
| Geometry | `rust/shell/src/koibody.rs` | Swept elliptical tube, caudal/dorsal/pectoral fins, kohaku |
| Runtime | `rust/shell/src/koirt.rs` | The drive model, fish senses, no sim |

### Swimming is not walking with different geometry

Three things separate this from the arthropods, and each is pinned by a test:

1. **Carangiform, not anguilliform.** The wave amplitude grows cubically toward
   the tail, so the head barely yaws and the tail sweeps widely. Applying the
   worm's uniform wave to a fish gives a swimming eel.
2. **Burst-and-coast.** Thrust only while beating, drag always. At cruise the
   fish alternates, and the glide is most of the duty cycle — which is what
   makes it look unhurried rather than mechanical.
3. **The C-start.** A startle bends the whole body one way at once — a standing
   bend, not a travelling one — then straightens explosively into a fast run
   away from the threat.

Depth (0 = deep, 1 = surfaced) drives apparent size and speed, because a flat
desktop has no other way to say a fish is at a different level.

## 5. Open

- **Zebrafish data status** needs checking against the current literature before
  anyone acts on §3.
- **The dorsal fin is nearly invisible from directly above**, which is
  anatomically right and visually a shame. A slight lean, or drawing it with a
  little thickness, would recover it.
- **No `--behaviortest` scenarios yet.** The fly's suite asserts sim→body
  behaviour; the koi has unit tests for its body and runtime but is not in the
  end-to-end suite, because that suite is built around a circuit driving a body
  and the koi has no circuit. Worth extending the harness rather than skipping.
