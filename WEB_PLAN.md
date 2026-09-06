# Creatures #5–#7: web-building spiders, and the silk they need

*Drafted 2026-09-06 on `spider-webs`, off `habitat-isometric`. Companion to
`SPIDER_PLAN.md` (the salticid, and the chimera rules this inherits),
`HABITAT_PLAN.md` (the enclosure, which is where webs start) and
`PORT_PLAN.md` §6 (what may and may not be claimed about a creature).*

**Status:** Phase 0 — the silk substrate — is built and verified (§9). Every
later phase is planned, not started.

## 0. TL;DR

- Jesse's call, 2026-09-06: *multiple spider species, each with its own
  distinct and accurate web-making logic.* Taken literally: the web is the
  **path the spider walks**, laid thread by thread in the sequence the
  ethology literature describes for that family, at a compressed tempo. Not a
  web texture that fades in behind a spider.
- **Three web families, three species, plus the salticid's retreat.** Each is
  a different architecture with a different construction program and a
  different way of catching prey, and that is the whole point of "multiple":
  an orb, a gumfoot tangle and a sheet-with-funnel do not share a rule set.

  | # | species | family | web | catches by |
  |---|---|---|---|---|
  | 5 | *Araneus diadematus*, garden cross spider | Araneidae | orb, rebuilt daily | sticky capture spiral; sits at the hub |
  | 6 | *Parasteatoda tepidariorum*, common house spider | Theridiidae | gumfoot tangle with central retreat, long-lived | tensioned sticky-footed lines that snap prey off the floor |
  | 7 | *Agelenopsis* sp., grass spider | Agelenidae | non-sticky sheet with a funnel retreat, thickened over days | vibration → a very fast rush from the funnel |
  | 3 | *Phidippus audax* (ships) | Salticidae | no capture web: a silk retreat and the dragline | vision (LC11), already shipped |

- **The honesty rules do not change.** No spider connectome exists (verified
  2026-09-05, `SPIDER_PLAN.md` §8.6). Each weaver is a labelled **chimera** on
  the fly's measured motor and escape modules, with **one authored node** (the
  strike) and a **procedural web-construction program that has no neurons in
  it at all** and is labelled as such. §3 turns that into tests.
- **What is new in engineering:** a silk model (a graph of threads the
  spider attaches, walks on, vibrates, cuts and repairs), a shared eight-leg
  rig pulled out of the salticid, three construction programs, and a thin-line
  renderer. **Phase 0 is the silk model, owed regardless of species, and it is
  done**: the salticid's dragline now runs through it with its suites
  byte-identical.
- Roughly **5–7 weeks** for all four spiders' silk, in eight phases, each with
  its own suite gate. The orb weaver alone is a shippable milestone (Phase 2).

## 1. What "accurate web-making logic" means here

Three readings, and the one taken:

- **(A) A drawn web.** A procedural web mesh appears behind the spider over a
  few seconds. Cheap, and not what was asked: it has no logic.
- **(B) A walked web** — *what this document assumes.* The spider builds the
  web the way the animal does: it walks, attaches, walks back, and each leg of
  that path *is* a thread. Zschokke's construction studies are literally
  recordings of the spider's path and activity over time, and reproducing the
  web means reproducing the path. This is what makes an orb weaver's
  half-built web look like an orb weaver's half-built web (a Y, then a wheel of
  radii, then a wide spiral, then a tight one) instead of a wireframe fading
  in.
- **(C) A biomechanical simulation** of silk — viscoelastic threads, wind
  loading, real tensions. Out of scope: the desktop cannot show it and it
  costs frame budget. Tension here is a scalar on a thread that decides when
  it breaks and how far a vibration travels, nothing more.

"Accurate" is bounded by what the literature actually records, and by
compression: *A. diadematus* takes roughly an hour to build an orb, half of it
on the capture spiral. Nobody will watch an hour. The **stage proportions and
the order are kept; the clock is compressed** (§5.1), and the compression
factor is one named constant.

"Distinct" is a claim about *families*, not colour schemes: the three programs
in §5 share the silk model and the leg rig and nothing else.

## 2. The species, and why these three

The filter from `PORT_PLAN.md` §6 still applies: **must not be creepy to have
on your screen all day.** A web spider is a harder sell than a jumping spider,
and glass anatomy is doing a lot of work. Each choice below is the least
threatening representative of its family that is also the best-documented
builder.

| species | why this one | creep risk | literature |
|---|---|---|---|
| ***Araneus diadematus*** | The single best-studied orb builder: Zschokke and Vollrath recorded its construction path stage by stage, and the nomenclature of the orb (hub, free zone, radii, frame, auxiliary and capture spiral) is defined on it. Round, marked abdomen, short legs — the "garden spider" most people already tolerate | low–medium | Zschokke & Vollrath (1995), Zschokke (1996), Vollrath — §12 |
| ***Parasteatoda tepidariorum*** | The theridiid whose gumfoot-web construction Benjamin & Zschokke (2003) described in detail: tangle first, repeated returns to a **central** retreat, then sheet, then the gumfoot lines. It is *the* common house spider, small and rounded, and — importantly — not a widow. *Latrodectus* has the same web type with a peripheral retreat and is not shipped, for the obvious reason | low | Benjamin & Zschokke (2003) — §12 |
| ***Agelenopsis* sp.** | The funnel weaver whose sheet is non-sticky, densely re-laid on every crossing, with a funnel retreat the spider waits in, front legs on the sheet, and rushes out of at speed. Striped and compact; the domestic *Tegenaria* alternative is longer-legged and fails the creep filter for more people | medium | Family-level accounts only so far (§12) — **read a primary source before Phase 4** |

Not shipped, and why: *Latrodectus* (a widow on the desktop is a different
product); *Nephila* (magnificent web, but a large long-legged body); cellar
spiders (*Pholcus*, the legs); any tarantula (no web logic to speak of).

## 3. Honesty, stated before the design

Everything `SPIDER_PLAN.md` §2 says still holds, and two things are new.

### 3.1 One chimera circuit, shared

The three weavers do not each get a bespoke circuit, because the honest
circuit is the same for all three: the fly's measured **motor and escape
modules** — looming (LC4/LPLC2), giant fiber (DNp01), steering (DNa01/02),
walking (DNp09), grooming (DNg11), backing up (MDN) — plus the fly's measured
**mechanosensory partners** (the wind/tap input that already exists, and that
already couples electrically onto GF). What differs by species is the body and
the program, not the wiring. So:

- **`data/weaver/`**, one directory, one `circuit.json`, produced by
  `etl_chimera.py --weaver`. The brain window and tray say *"shared weaver
  chimera"*, and the README's table counts it once.
- **LC11 is dropped.** Orb weavers and theridiids are close to blind; their
  prey sense is vibration. Shipping a visual small-object pathway inside an
  animal that does not hunt by sight would be a lie with real data in it. The
  salticid keeps LC11; the weavers do not have it.
- **Prey is vibration**, transduced onto the measured mechanosensory neurons
  the way bugs are transduced onto LC11: the transduction (thread excitation
  at the spider's legs → input current) is modelled and labelled; everything
  downstream of those neurons is measured wiring.
- **The measured wiring says a vibration is a threat.** The mechanosensory
  partners drive GF, and GF is escape. That is *correct* for a large
  vibration — an orb weaver hit by something big drops from the hub on its
  dragline — and wrong for a struggling fly. So the amplitude split is where
  the one authored node lives: **`strike`**, a node with authored edges from
  the mechanosensory population, that integrates *sustained, small-amplitude*
  excitation and, above threshold, is the body's cue to go to the prey. Its
  provenance is cool-coloured in the brain window exactly as `pounce` is.
- **Localising the prey** — which radius, which gumfoot line — is a modelled
  readout of per-leg excitation, in the same category as the salticid's
  head-orient readout of LC11's left−right rate, and labelled the same way.

### 3.2 The web program has no neurons in it, and says so

Web construction is a stereotyped motor program with **no counterpart
anywhere in the fly**, and no spider circuit to run. So it is procedural — the
koi's `Provenance::Procedural` claim, applied to one behaviour inside a chimera
rather than to a whole animal. The rules:

1. `Provenance::Chimera` gains a `procedural: Vec<String>` field listing the
   behaviours with no neurons in the loop. For a weaver it is
   `["web construction program"]`. `describe()` prints it: *"chimera: FlyWire
   FAFB v783 modules + authored strike connective; web construction:
   PROCEDURAL"*. For the salticid the list is empty and its output is
   unchanged.
2. The program **may read** brain signals — it runs only when the measured
   walk drive says the animal is active and the circadian curve says it is
   night (orb weavers build at dusk) — and it **never writes** them. The brain
   decides *whether* the spider is moving; the program decides *where the
   thread goes*. A test asserts the program touches no sim input.
3. The words *"spider connectome"* and *"spider brain"* never appear; the test
   from `SPIDER_PLAN.md` §2 rule 4 runs on all three weavers.
4. The README's measured-vs-modelled table gets one row per new claim: the
   strike node (count from the data), the vibration transduction, the
   localisation readout, and the construction program.

## 4. The silk model (Phase 0 — built)

`rust/core/src/silk.rs`. A `Silk` is a graph:

- **Nodes** are points with an `Anchor`: `Fixed` (a wall, a ledge, a prop, the
  floor — something outside the web) or `Free` (a junction the spider made by
  attaching one thread to another, which moves if the web is pulled). Each
  carries a scalar **excitation** — the vibration field.
- **Threads** join two nodes and carry a `ThreadKind` — `Dragline`, `Bridge`,
  `Frame`, `Radius`, `Auxiliary`, `Capture`, `Tangle`, `Gumfoot`, `Sheet`,
  `Retreat` — a `sticky` flag, a rest length and a tension. The kind is what
  the renderer colours by and what the programs reason about ("follow the
  next auxiliary loop inward").
- **The trailing line.** The spinnerets are always the free end of the thread
  in progress: `pay_out(at)` starts a line from a new node; the spider walks;
  `attach(at)` closes it into a thread and starts the next one from there.
  That two-call rhythm *is* web construction, and it is also the salticid's
  dragline (`pay_out` before a jump, `release` on landing). **The salticid now
  uses it**, and its suites and snapshots are byte-identical (§9).
- **Vibration.** `excite(at, amount)` deposits energy at the nearest node;
  `step(dt)` spreads it along threads scaled by tension and damps it. Prey and
  the cursor excite; the spider's legs read the excitation of the nodes it
  stands on. Two constants, and a test that a pulse at one end of a radius
  arrives at the hub and decays.
- **Damage.** `cut_near(p, r)` severs threads within a radius — a cursor
  sweep, a prop rolling through, a window edge moving. A severed thread is
  gone; the nodes it left are the program's repair work list.
- **Rendering** stays with the creature that owns the silk: `segments()`
  yields (a, b, kind, excitation) for every live thread, plus the trailing
  line to the spider's position, and the shell draws them. Phase 0 draws them
  the way the dragline was drawn (a thin capsule each); Phase 7 replaces that
  with a line pipeline once there are a thousand of them.

What it deliberately is not: a physics simulation. Tension is set by the
program when the thread is laid and only ever relaxes; nothing sags, nothing
blows. If wind ever matters, it is an excitation source, not a force.

## 5. The programs

Each program is a state machine inside its species' `Body`. It reads the
silk, the region (anchors), the brain signals and the circadian phase, and it
emits walking targets and `pay_out`/`attach`/`cut` calls. Timings are
literature proportions under one compression constant, `WEB_TEMPO`
(default: a full orb in ~5 minutes; 1.0 would be real time).

### 5.1 Orb — *Araneus diadematus*

The sequence, from Zschokke (1996) for the early stages and Zschokke &
Vollrath (1995) for the rest:

1. **Exploration and bridge.** Walk the anchors, trailing a line; the first
   thread that spans a gap is the bridge. Zschokke found this stage the most
   variable — a series of fixed behavioural patterns in seemingly random
   order — and the program reproduces that by choosing among a small set of
   moves at random until a bridge exists.
2. **Proto-hub and the Y.** Drop from the bridge's midpoint on a line, attach
   below, tighten: the first three radii meet at the proto-hub.
3. **Frame and primary radii.** Walk out along a radius, attach to the frame
   or an anchor, return to the hub — every radius is one out-and-back. Frame
   threads are laid on the way. Vollrath's radius-orientation result is the
   rule used: the next radius goes into the **largest remaining angular
   gap**, alternating sides, so the wheel fills evenly rather than in order.
4. **Remaining radii** by the same rule until the gap falls below a species
   minimum (roughly 25–35 radii for this species).
5. **Auxiliary spiral**, from the hub outward: few turns, wide pitch, laid
   while walking the radii. This is the scaffold.
6. **Capture spiral**, from the periphery inward, sticky: the spider follows
   each auxiliary loop for several capture loops before breaking that loop and
   moving to the next one in — the "bundling" Zschokke & Vollrath recorded —
   and cuts the auxiliary as it goes. This is half the total time and it
   stays half.
7. **Hub finish.** Bite out the centre of the hub, leaving the free zone.
8. **Sit**, head down at the hub, legs on radii.

Prey: excitation on a radius → `strike` → run out along that radius, brief
wrap at the site, carry back, feed (stillness). Threat: a loom or a large
excitation → GF → **drop from the hub on the dragline**, hang, climb back —
the salticid's abseil, re-used. Rebuild: at the next dusk the spider cuts and
consumes the old web (real: this species rebuilds most days) and starts again.
Damage below a threshold is repaired (re-lay the missing radii); above it, the
rebuild comes early.

### 5.2 Gumfoot tangle — *Parasteatoda tepidariorum*

From Benjamin & Zschokke (2003), whose construction sequence for this species
is: tangle before sheet, repeated returns to the retreat while building the
tangle, alternation between sheet and tangle, and the gumfoot lines last.

1. **Retreat**, central, high — under a ledge or the lid of the terrarium.
2. **Tangle**: lines from the retreat out to anchors and back, and between
   each other, forming the three-dimensional mesh. The spider goes home
   between bouts.
3. **Sheet** below the tangle; then more tangle; alternate.
4. **Gumfoot lines**: from the tangle straight down to the floor, attached
   under tension, with a sticky foot. Several, spread around the retreat.
5. **Sit** in the retreat, upside down, a leg on a line.

Prey: something walking on the floor touches a foot; the foot breaks, the
tensioned line **snaps upward and lifts the prey** — the one moment this web
does something visibly mechanical — the spider descends, wraps, hauls it up.
Threat: retreat deeper into the retreat; a large disturbance → drop and hang.
The web is **not rebuilt**: theridiid webs persist and grow; each night adds
gumfoot lines and tangle, and damage is repaired by adding, not replacing.

### 5.3 Sheet and funnel — *Agelenopsis*

Family-level description, to be checked against a primary source before it is
coded (Phase 4 gate):

1. **Funnel** first, against a wall or behind a prop: a tube of dense silk.
2. **Sheet** out from the funnel mouth: a flat, slightly concave mesh, laid
   as the spider walks over the area again and again — every crossing adds a
   thread, so the sheet is never "finished", it thickens.
3. **Guy lines** above the sheet to anchors, which are what entangle prey.
4. **Sit** in the funnel mouth, front legs on the sheet.

Prey: excitation anywhere on the sheet → `strike` → **rush** (this family is
fast: the burst is the highest walking speed of any creature in the app),
bite, drag back into the funnel. Threat: back into the funnel, never drop.
Damage: crossed over and re-threaded as part of ordinary movement; there is
no separate repair program, which is itself accurate.

### 5.4 The salticid's retreat

Not a capture web and not a new creature: *Phidippus* builds a silk retreat
("pup tent") in a crevice to sleep and moult in, and the dragline is a jump
stabiliser as much as a safety line (Chen et al. 2013). The existing spider
gains: a `Retreat` thread cluster built in a corner over a couple of minutes,
`Sleeping` moved inside it, a return-to-retreat walk at the circadian dusk,
and nothing else. This is the smallest phase and it lands first after the orb.

## 6. Where the anchors come from

Webs need fixed points, and the desktop has two kinds.

- **Habitat first.** A terrarium has walls, a floor and props that do not
  move unless the creature moves them. Every program is developed and tested
  against `Region` walls and `Prop` positions, and the weavers ship with the
  habitat **on by default for them** (the tray still allows free roam).
- **Free roam later (Phase 6).** Window top edges and screen edges are the
  anchors, so an orb spans the gap between two of your windows. Moving a
  window then *cuts the threads attached to it*, and the spider repairs or
  rebuilds. That is real behaviour and the plan treats it as the feature it
  is, not as a bug to hide — but it is also why it comes after the programs
  are known to work in a stable box.

Nothing here reads window content; anchors are the same `Ledge` geometry the
fly stands on.

## 7. Architecture

| piece | file | phase |
|---|---|---|
| silk graph, trailing line, vibration, damage | `rust/core/src/silk.rs` | **0 ✅** |
| eight-leg rig and tetrapod gait, extracted from the salticid | `rust/core/src/arachnid.rs` | 1 |
| shared weaver chimera circuit, `strike` node | `etl_chimera.py --weaver` → `data/weaver/` | 1 |
| `Provenance::Chimera { procedural }`, label tests | `rust/core/src/creature.rs` | 1 |
| `Substrate::WalkerWeaver` | `creature.rs`, `habitat.rs` | 1 |
| orb body and program | `rust/core/src/orb.rs` | 2 |
| gumfoot body and program | `rust/core/src/cobweb.rs` | 3 |
| sheet body and program | `rust/core/src/funnel.rs` | 4 |
| vibration transduction, strike readout, one runtime for all three | `rust/shell/src/weaverrt.rs`, `transduction.rs` | 2 |
| thin-line pipeline, per-kind colour, excitation glow | `rust/shell/src/silkmesh.rs`, `shader.wgsl` | 7 |
| three glass bodies | `rust/shell/src/orbbody.rs` … | 2–4 |

Two seams matter. `Silk` is owned by the body, not the runtime, because the
program that writes it and the legs that read it are the same object. And the
three weavers share **one** `Runtime` implementation parameterised by species,
because their senses, circuit and readouts are identical; only `Body` differs.

## 8. Phased plan

Each phase ends with `cargo test --workspace` green and both fly suites and
both salticid suites byte-identical. Estimates are working days.

### Phase 0 — the silk substrate (2 days) — ✅ done 2026-09-06

`silk.rs`; the salticid's dragline and abseil line re-expressed as a trailing
line on a `Silk`; the shell draws whatever the silk holds. **Gate met:** the
salticid's `--simtest --behaviortest` output and both its snapshots are
byte-identical to the parent commit; the fly's likewise. §9 has the numbers.

### Phase 1 — plumbing (3–4 days)

`arachnid.rs` leg rig extracted with the salticid byte-identical; the weaver
circuit from the ETL with its in-degree report read (**gate:** in-circuit
drive onto the mechanosensory population from the shipped partners is what it
already is for the fly, and the strike node's authored in-degree is reported);
`Provenance::Chimera.procedural` and its tests; `WalkerWeaver`; a static glass
*Araneus* in the snapshot path **before** any program is written, to run the
creep filter on the body alone.

### Phase 2 — the orb weaver (6–8 days) — the shippable milestone

`orb.rs` with the eight stages of §5.1; `weaverrt.rs`; vibration
transduction; drop-and-climb on threat; prey by vibration; daily rebuild.
**`--behaviortest` scenarios:** a full build completes inside the terrarium in
under `WEB_TEMPO` minutes with radius count in the species band and the
capture spiral inside the auxiliary; excitation on a radius → strike → the
spider reaches the site along *that* radius within 2 s; a loom at the hub →
GF → drop on the dragline, no snap on return; a cut through half the radii →
repair or early rebuild; the program never writes a sim input.

### Phase 3 — the gumfoot weaver (5–6 days)

`cobweb.rs` per §5.2, the snapping gumfoot line, additive nightly building.
**Scenarios:** tangle exists before sheet; the spider returns to the retreat
between tangle bouts; a bug on the floor touching a foot is lifted and caught;
the web survives a day boundary and grows.

### Phase 4 — the funnel weaver (5–6 days)

**Gate before coding:** a primary source for agelenid construction, cited in
§12. Then `funnel.rs`, the rush, the thickening sheet. **Scenarios:** sheet
density rises monotonically with crossings; strike → rush speed is the
app-wide maximum and the return goes into the funnel; threat never drops.

### Phase 5 — the salticid's retreat (2 days)

§5.4. **Gate:** the salticid suite output changes only by the new scenario's
lines; everything above them is byte-identical.

### Phase 6 — free-roam anchors (3–4 days)

Ledges and screen edges as anchors; window movement as damage; the tray's
habitat default per creature. **Scenario:** an orb spanning two ledges loses
the threads on a ledge that moves, and only those.

### Phase 7 — rendering and budget (3–4 days)

Line pipeline; excitation as a glow that travels; `--snapshot` for each
weaver mid-build checked into `assets/`; a thirty-second run of the orb
weaver mid-capture-spiral at the 30 fps budget on the reference machine.

## 9. Phase 0 — what was built and how it was checked

*Recorded 2026-09-06 as the phase closed.*

- **`rust/core/src/silk.rs`**: `Silk` (nodes with `Anchor::Fixed | Free` and
  an excitation scalar; threads with a `ThreadKind`, rest length and
  tension), the trailing line (`pay_out` / `attach` / `attach_to` /
  `set_kind` / `release`), damage (`cut_near`, which prunes orphaned nodes and
  re-indexes so the trailing anchor survives), vibration (`excite`,
  `excitation_at`, `step`), and `segments()` for the renderer. Six unit tests:
  a released dragline leaves nothing; walking-and-attaching lays a chain and
  a junction; a cut removes only what it crosses; a pulse travels a chain and
  dies away; paying out again lets the old line go; the sticky kinds.
- **The salticid's dragline is now the trailing line.** `Spider.dragline`
  (a bare `Option<Vec2>`) became `Spider.silk`, with `dragline()` as the
  accessor the suites and tests read. `start_jump` and `start_abseil` call
  `pay_out`; `land` and the end of a climb call `release`; the escape from a
  hanging abseil re-pays-out from the abseil anchor as before.
  `SpiderRuntime::moved_display` clears the silk. The renderer draws
  `silk.segments(pos)` in a loop instead of one special-cased line, with the
  same capsule and the same transform, so the only line that exists today is
  drawn exactly as it was.
- **Checked against the parent commit (473aa4a)**, built to a separate target
  directory because a running copy of the app holds the release binary:

  | check | result |
  |---|---|
  | fly `--simtest --behaviortest` | byte-identical |
  | salticid `--simtest --behaviortest` (14 body checks) | byte-identical |
  | `--snapshot` fly, glass and literal | byte-identical |
  | `--snapshot` salticid, glass and literal | byte-identical |
  | `cargo test --workspace --exclude dfplatform` | 207 passed (201 before + 6 silk) |

  `dfplatform` is excluded for the reason `HABITAT_PLAN.md` §4 gives: its
  named-pipe test cannot pass while the app owns the pipe.
- **Not done, deliberately:** no colours per thread kind beyond a brighter
  alpha for sticky kinds (Phase 7), no line pipeline (Phase 7), no anchors
  from the habitat or from ledges (Phases 2 and 6). The silk has no consumer
  that lays more than one thread yet; the tests are what prove it can.

## 10. Decisions taken 2026-09-06

1. **Reading (B):** the web is the walked path.
2. **Three families, one species each**, as §2. The widow is not shipped.
3. **One shared weaver circuit** in `data/weaver/`, not three copies of a
   26,000-edge extract with different names.
4. **LC11 is dropped from the weavers.** A blind hunter does not get a visual
   prey pathway because it happens to be lying around.
5. **The strike is an authored node**, for the same reason the pounce is:
   one visibly invented thing to point at.
6. **The construction program is procedural and labelled**, through a new
   field on `Provenance::Chimera` rather than a fifth variant, because the
   animal *is* still a chimera; one behaviour inside it has no neurons.
7. **Habitat first, free roam in Phase 6.** Programs are proven in a box with
   fixed walls before they meet windows that move.
8. **Real time is compressed by one constant**, proportions kept.
9. **The salticid's dragline moves onto the silk model now** (Phase 0), so
   there is one line implementation, checked by byte identity, rather than
   two that drift.

## 11. Risks

| risk | severity | mitigation |
|---|---|---|
| Web spiders fail the creep filter even as glass | **High** | Phase 1 renders the static *Araneus* before any program exists; if it fails, stop at the salticid's retreat |
| A thousand threads blow the 30 fps budget | Medium | Capsules in Phase 0 are a placeholder; Phase 7's line pipeline is one draw; measure at the end of Phase 2 |
| "Accurate" drifts into hand-waving for the funnel weaver | Medium | Phase 4 gate is a primary source; family-level web pages are not enough to code from |
| The strike node fires on threats, or GF on prey | Medium | The amplitude split is a suite scenario in both directions from Phase 2 |
| The program quietly drives the brain | Medium | Rule 3.2.2 is a test: the program's inputs are read-only |
| Silk on window ledges is occlusion | Medium | Free roam is Phase 6, off by default for weavers; the tray says so |
| A lot of new creature surface for one person to maintain | Medium | One runtime, one leg rig, one silk model; only the programs are per species |

## 12. Sources

Checked 2026-09-06. Primary where available; the agelenid entries are not,
and §8 Phase 4 gates on fixing that.

- Zschokke, S. & Vollrath, F. (1995). *Web construction patterns in a range
  of orb-weaving spiders (Araneae).* European Journal of Entomology 92:
  523–541. — stage sequence and durations; capture-spiral "bundling" against
  the auxiliary spiral.
  https://www.researchgate.net/publication/256375675_Web_construction_patterns_in_a_range_of_orb-weaving_spiders_Araneae
- Zschokke, S. (1996). *Early stages of orb web construction in Araneus
  diadematus Clerck.* Revue suisse de Zoologie, hors série: 709–720. — the
  variable early stage; proto-hub; frame and primary radii before radius
  filling.
  https://www.semanticscholar.org/paper/06890f48e1a129439871b912ef219de7b1b32752
- Zschokke, S. *Nomenclature of the orb-web.* — the terms used in §5.1.
  https://www.researchgate.net/publication/256375605_Nomenclature_of_the_orb-web
- Vollrath, F. *Radius orientation in the cross spider Araneus diadematus.*
  — the largest-gap rule for radius placement.
  https://european-arachnology.org/esa/wp-content/uploads/2015/08/107-116_Vollrath.pdf
- Benjamin, S. P. & Zschokke, S. (2003). *Webs of theridiid spiders:
  construction, structure and evolution.* Biological Journal of the Linnean
  Society 78(3): 293–305. — *Parasteatoda* (then *Achaearanea*) construction
  sequence; central versus peripheral retreat.
  https://academic.oup.com/biolinnean/article-abstract/78/3/293/2639734
- Chen, Y.-K., Liao, C.-P., Tsai, F.-Y. & Chi, K.-J. (2013). *More than a
  safety line: jump-stabilizing silk of salticids.* Journal of the Royal
  Society Interface 10: 20130572. — the dragline as an in-air stabiliser.
  https://royalsocietypublishing.org/doi/abs/10.1098/rsif.2013.0572
- Agelenidae, family accounts (secondary; **not sufficient to code from**):
  https://www.inaturalist.org/taxa/47345-Agelenidae ,
  https://en.wikipedia.org/wiki/Agelena
