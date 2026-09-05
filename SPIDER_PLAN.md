# Creature #3: a coding spider

*Drafted 2026-09-05, the day `windows-port-plan` merged to master. Companion to
`PORT_PLAN.md`, which this assumes you have read — especially §5 (the creature
seams), §6.2 (invented creatures and the honesty rules) and §6.3 (what is fun).*

## 0. TL;DR

- **A jumping spider that lives on your desktop while you code**: it orients its
  head to your cursor, stalks and pounces on "bugs" that appear when a build or
  test run fails, abseils off window ledges on a dragline, and jumps away from a
  lunge. Glass-anatomy rendering, so you watch its circuit fire inside it.
- **No spider connectome exists** (as far as I know — verify, §2). So this is the
  **chimera** creature that `PORT_PLAN.md` §6.2(ii) recommends as creature #3:
  every circuit inside it is measured FlyWire data; the animal itself does not
  exist, and the UI says so structurally, not in a footnote.
- **The new science is one population**: FlyWire's **LC11 small-object motion
  detectors** give the spider a real prey-detection pathway. Everything else it
  needs (looming → giant fiber, DNa steering, DNp09 walking, DNg11 grooming,
  MDN backing up) the fly already ships.
- **The new engineering is mostly body and senses**: an eight-legged tetrapod
  walker with a ballistic jump and a dragline, and a small, permission-free,
  content-blind set of "coding" senses (which app is in front, typing cadence,
  an opt-in build-result hook).
- **Before any of it: the shell had no creature picker.** The worm from Phase 5
  existed only in `dfcore`; `rust/shell/src/main.rs` constructed `LifSim` and
  `Fly` by name. That was Phase 0 here, owed regardless, and it is **done**
  (§7).
- Roughly **3–4 weeks**, in six phases, each with its own suite gate.

## 1. What "coding spider" means here

Two readings are possible, and they lead to different work:

- **(A) A spider that reacts to your coding** — *what this document assumes.*
  The pet's senses extend from "is the user here / typing / moving the cursor"
  to "is the user in an editor, did the build just fail". The spider's signature
  behaviour — sit, watch, stalk, pounce — maps onto that better than any other
  animal's would.
- **(B) A spider that writes code.** A pet that emits text into your editor is a
  different product, needs keystroke injection, and breaks the app's "knows
  when, never what" claim. Not planned; say so if you meant it.

The species is a **jumping spider (Salticidae)**, and it matters which spider:

- It is the one spider most people find *charming* — large forward-facing
  principal eyes, a fuzzy compact body, and the head-turn it makes to look at
  you. This is the only way a spider passes `PORT_PLAN.md`'s filter #2
  ("must not be creepy to have on your screen all day"). A web-building orb
  weaver or a long-legged cellar spider would fail it, and no rendering saves
  them.
- Glass anatomy (now the default renderer) does the rest: no hair, no chitin,
  a soft translucent body with the circuit visible inside.
- Its real behaviours are the ones a desktop pet wants: visual prey pursuit,
  a ballistic jump with a **dragline always attached first**, abseiling on that
  line, an alternating tetrapod gait, grooming, and long stationary watching.
  Salticids do **not** build capture webs — so no web across your windows,
  which is a feature: a web would be occlusion, and a dragline is a single
  line.

## 2. The honesty problem, stated before the design

`PORT_PLAN.md` §6 has two hard filters. This creature fails the first one
outright: **there is no complete, published, synapse-resolution spider
connectome.** Spider neuroscience is real and rich (salticid vision in
particular — the principal-eye retinal scanning, the secondary eyes as motion
detectors that trigger the orienting turn), but it is physiology and behaviour,
not wiring. *Verify this before Phase 1; if an EM spider dataset has appeared
since, the plan changes shape and gets better.*

So the spider is built the way §6.2(ii) prescribes: **real circuit modules,
recombined.** And that runs straight into the plan's own rule 4:

> **Never fabricate a real species.** A fake "honeybee connectome" would be a
> straightforward scientific-integrity failure.

The line between "a chimera with a spider silhouette" and "a fake spider brain"
is thin, and it is drawn entirely by what the app *claims*. These are the
rules, and Phase 1 turns them into code and tests rather than prose:

1. **It is never called a spider brain.** Display name: *"Jumping spider —
   chimera"*. Provenance line in the tray and brain window: *"No such animal.
   Vision, escape, steering and walking circuits: FlyWire FAFB v783 (measured).
   Body and connectives: authored."* The word "spider" never modifies the word
   "connectome" anywhere in the UI, README or data.
2. **Provenance moves to per-neuron and per-edge.** `Connectome.provenance` is
   one field today. The chimera mixes at the edge level (a measured LC11 axon
   onto an authored connective), so honesty has to live at that granularity.
   This is the §6.2 rule 1 refactor, finally paid for.
3. **The brain window colours by provenance.** Measured neurons and edges warm,
   authored ones cool, at a glance. Measured modules keep their real FlyWire
   soma coordinates; authored connectives are laid out force-directed between
   the modules they join, so they *look* invented.
4. **A test enforces the labels.** `Provenance::describe()` for this creature
   must contain "chimera" and must not contain "spider connectome" or "spider
   brain"; every edge must carry a provenance; the count of authored edges is
   reported in the README's measured-vs-modelled table, next to the LIF and
   habituation entries.

Is this still "a cosmetic skin", the option (C) that `PORT_PLAN.md` argued
against? Partly, and it is worth being plain about it: the escape, steering and
walking modules are the fly's, and a spider body over an unchanged fly circuit
*would* be a skin. What makes it a chimera rather than a skin is that the module
set is different (adds the LC11 prey pathway, drops the wing modules), the
composition contains connectives that exist in no animal, and the UI says all of
this. If at the end of Phase 2 the authored part is empty and the module set is
the fly's, stop and call it a skin honestly, or don't ship it.

## 3. The circuit: modules and provenance

| module | FlyWire type(s) | role slug | provenance | drives (spider) |
|---|---|---|---|---|
| looming detectors | LC4, LPLC2 | `lc4`, `lplc2` | measured, shipped | nervous crouch; excite GF |
| giant fiber | DNp01 | `gf` | measured, shipped | **escape jump** (spike = jump away, dragline attached) |
| steering | DNa01, DNa02 | `dna01`, `dna02` | measured, shipped | L−R rate → turn bias; also the **head-orient turn** toward a target |
| forward walking | DNp09 | `dnp09` | measured, shipped | walk/rest hysteresis, stalking speed |
| grooming | DNg11 | `dng11` | measured, shipped | leg/palp grooming |
| backward walking | MDN | `mdn` | measured, shipped | back away (spiders do) |
| **small-object detectors** | **LC11** | **`lc11`** | **measured, new** | **prey detection** → pursuit drive, orient toward the object |
| pursuit → pounce connective | — | `pounce` | **authored** | integrates LC11 rate and target range; above threshold, the body pounces |
| dragline decision | — | (body) | modelled, not neural | attach before any jump; abseil when leaving a ledge downward |
| ascending proprioception | strongest ascending partners | `other` | measured, shipped | gait phase back into the network (already closed for the fly) |
| wind / tap | strongest sensory partners | `other` | measured, shipped | clicks as substrate taps |
| ~~wing effort~~ | ~~DNp02/04/11~~ | ~~`escw`~~ | — | **dropped**: no wings |

Notes that will bite if ignored:

- **LC11 is the one population to add, and it follows `CLAUDE.md`'s recipe
  exactly.** Step 1 is `grep -c ',LC11,'` on `consolidated_cell_types.csv.gz`;
  step 3 is the in-degree report: if in-circuit drive onto `lc11` is not at
  least several hundred synapses, the population will be noise-driven and prey
  pursuit will be random walking with a story attached (this shipped once for
  DNg11 at 6 synapses). The raw dumps are **not on this machine** — budget the
  download.
- **Do not relabel GF as the pounce.** GF is the escape command; a jumping
  spider's escape jump is a legitimate reading of it. The *predatory* pounce has
  no measured counterpart in the fly, so it is the one authored connective in
  the circuit, and it is cool-coloured in the brain window. That is the point
  of the chimera: one honest seam, visible.
- **LC11 → steering is real wiring, if it exists in the extract.** The ETL's
  reserved-partner loop will pull LC11's strongest in-circuit targets; whether
  they reach DNa01/02 directly is a question for the data, not for the plan.
  If they do, orienting toward prey is measured end to end. If they don't, the
  orient turn goes through the authored connective too, and the table above
  gets one more cool edge.
- **The operating point is still razor-thin.** LC11 gets the same fixed command
  baseline discipline as everything else (deterministic, never per-side random);
  the `--simtest` invariants (GF silent over 4 s of rest, GF ≤ ~10 ms after an
  abrupt loom) must still pass on the chimera circuit, not just the fly's.

## 4. The body

A new `Body` implementation in `rust/core/src/spider.rs`, and a new
`Substrate::WalkerJumper`: walks on the desktop and on window ledges, can jump
between them ballistically, cannot fly. Nothing in `body.rs` (the fly) or
`worm.rs` changes.

**Locomotion.** Eight legs, alternating tetrapod gait: legs 1 and 3 on one
side step with 2 and 4 on the other, then the complement. One gait-phase scalar
drives all eight, exactly as the fly's tripod does, and feeds proprioception
back into the ascending neurons the same way. Stalking is DNp09 walking at low
drive with a lowered body; that is a rendering of the same signal, not a new
state.

**States.** `Watching` (the default and the characteristic one — a salticid
spends most of its time stationary, turning its cephalothorax to track things),
`Walking`, `Stalking`, `Pouncing` (ballistic, ~150 ms, dragline attached),
`Abseiling` (hanging from a ledge on the line, descending, climbing back),
`Grooming`, `Sleeping`. Same discipline as the fly: hysteresis plus the ≥0.4 s
`state_age` dwell guard on transitions, cooldown timers on one-shots, and every
one-shot must work from every grounded state (MDN was once dead from idle).

**Head orient.** The single most engaging thing this creature does, and the
cheapest: the cephalothorax yaws toward whatever LC11 or the looming detectors
are responding to, before the body moves. Driven by the DNa L−R rate, slew
limited so it reads as a look, not a snap.

**Jump.** Both the escape jump (GF spike) and the pounce (authored connective)
are the same body mechanic with different targets: attach dragline at the
current point, ballistic arc, land, retract. Landing goes through a short
crouch-and-settle — never snap scale or z, the same rule as the fly's flare.

**Dragline and abseil.** Leaving a ledge downward is not a fall: the spider
lowers itself on the line and can climb back. Rendered as one thin line from the
ledge to the spider. This is the behaviour that makes it *live on the windows*
rather than beside them.

**Prey.** "Bugs" are small autonomous sprites the shell spawns (§5): a few
points of drifting motion the spider's LC11 pathway can see. The spider's
transduction turns each bug's angular size and velocity into LC11 drive, the
circuit does the rest, and a pounce whose landing point is within range of a bug
removes it. A caught bug is the *only* moment the spider produces a visible
reward cue — keep it to a brief brighten of the circuit.

## 5. The coding senses

The fly's rule is *knows when, never what*, and it is permission-free. The
spider keeps both, and adds three senses, all read through `dfplatform` into
`EnvSnapshot` like the existing six scalars:

| sense | how | what the spider does |
|---|---|---|
| **foreground app class** | process name of the foreground window (editor / terminal / browser / other), never its title or content | in an editor or terminal it settles into `Watching` near the window's edge; elsewhere it wanders |
| **typing cadence** | already inferred on Windows (recent input + stationary cursor); the spider reads bursts and pauses | a long burst then a pause is when it looks at you |
| **build / test outcome** | **opt-in hook only**: a `desktopfly notify pass` / `notify fail` subcommand you put in a script, git hook or task; nothing is watched without being asked | `fail` spawns bugs near the terminal; `pass` after a fail is when it goes back to watching. Streaks are not scored — this is a pet, not a dashboard |

What it deliberately **does not** know: which keys, which file, which repo, what
the error said, what site the browser is on. The hook is the entire build
integration; there is no log tailing and no file watching, because those read
content.

**Habituation applies here too.** Repeated harmless cursor approaches stop
startling it (that already ships); repeated build failures do not habituate —
bugs are prey, not threats, and a spider that stops hunting is a worse pet.
Dishabituation on a real lunge is already in the transduction layer.

## 6. Phase 0 first: the shell has no creature picker

Phase 5 of the port delivered the worm as a `Creature`, `Body` and manifest in
`dfcore`, with its own suite. It delivered **nothing in the shell**: no
`--creature` flag, no tray entry, and `main.rs` still builds `LifSim` and `Fly`
by name. The traits exist; the shell does not use them.

That is fine for a data-less worm, but it means creature #3 cannot appear on a
desktop until the shell is generic over `Sim`, `Body` and the creature's
renderer. This is the §5 seam work applied to the last place it was skipped,
and it is Phase 0 here rather than a note because every later phase depends on
it.

## 7. Phased plan

Each phase ends with `cargo test --workspace` green and both ground-truth
suites unchanged for the fly. Estimates are working days.

### Phase 0 — creature picker in the shell (2–3 days) — ✅ done 2026-09-05

- `--creature drosophila|c_elegans` and a tray *Creature* submenu; the choice
  is persisted in `settings.json` next to the habituation files, which are
  now **per creature**.
- One seam rather than three: `rust/shell/src/runtime.rs` puts brain, body,
  senses→stimulus, rates→commands *and* geometry behind a `Runtime` trait,
  because those five vary together per creature and splitting them would have
  meant three trait objects that only ever travel as a set. `FlyRuntime` is
  the old `main.rs` code moved, not rewritten; `wormrt.rs` is the worm's, with
  touch-only senses and a `GradedSignalBuilder` in core for its readout.
- The worm renders as a tapered glass capsule tube (`wormbody.rs`), brainless
  until its data ships.
- **Gate met:** the fly's `--snapshot` (glass and literal) is byte-identical
  to the pre-seam build through the generic path; all 122 tests pass; both
  creatures run live and exit cleanly.

### Phase 1 — chimera plumbing (3–4 days) — ✅ done 2026-09-05

- `Provenance` per neuron and per edge; `Connectome` carries it; the loader
  defaults every FlyWire row to measured so the fly's data files are untouched.
- Brain window colours by provenance; authored nodes are laid out force-directed
  between the modules they connect.
- `Provenance::describe()` and the label tests from §2 rule 4.
- **Gate:** the fly reports 0 authored edges and renders exactly as before.

### Phase 2 — LC11 and the chimera circuit (2–3 days, plus the download) — ✅ done 2026-09-05

- Fetch the v783 dumps; `grep -c ',LC11,'`; add `"LC11": "lc11"` to
  `CORE_TYPES` and to both partner loops; rerun the ETL and **read the
  in-degree report**.
- `etl_chimera.py`: takes the fly's `circuit.json`, drops `escw`, keeps LC11,
  appends the authored `pounce` connective with explicit provenance, and writes
  `data/salticid/`. Same licence split: the measured part stays CC BY-NC 4.0.
- Manifest `roles::salticid()`; `Creature` impl; `DynamicsSpec::Lif` (the
  modules are spiking fly neurons, so the integrator is honest).
- `--simtest` on the chimera: the fly's invariants plus *LC11 silent at rest,
  LC11 fires on a small moving object, LC11 does not fire on a full-field loom*
  (that is the small-object selectivity the population is known for; if the
  extract does not reproduce it, say so in the README rather than tune until
  it does).
- **Gate:** in-circuit drive onto `lc11` ≥ several hundred synapses, or the
  population is cut and the pursuit goes fully authored — and labelled so.

### Phase 3 — the spider body (4–6 days) — ✅ done 2026-09-05

- `spider.rs`: tetrapod gait, states, head orient, jump with dragline, abseil,
  prey capture. `Substrate::WalkerJumper`.
- Proprioception closes the loop into the ascending partners as the fly's does.
- **`--behaviortest` scenarios** (the suite is the ground truth): stimulate
  `lc11` → head orients within 300 ms and the body enters `Stalking`; abrupt
  loom → GF → escape jump with dragline attached, no scale/z snap at landing;
  bug within pounce range under pursuit drive → capture within 3 s; `mdn` burst
  → backing up from every grounded state; `dng11` → grooming; walk-drive duty
  in the fly's 20–50% band under the same drive; siesta walk-drive > 3%.

### Phase 4 — coding senses (3–4 days) — ✅ done 2026-09-05

- `EnvSnapshot` gains `foreground_class` and `build_event`; Windows
  implementation in `windows_senses.rs`; the macOS build gets the same fields
  via `Senses::fidelity_notes()` until it is wired.
- `desktopfly notify pass|fail` over a local named pipe (Windows) / Unix socket.
- Bug spawner in the shell; transduction of bugs into LC11 drive.
- **Gate:** a `senses.exe` run shows the class changing as you alt-tab, and a
  `notify fail` from another terminal spawns bugs.

### Phase 5 — rendering (4–5 days) — ✅ done 2026-09-05

- Glass salticid mesh: cephalothorax, abdomen, eight legs on the gait phase,
  the principal eyes as the one place the glass is denser. Circuit points
  mapped into the body as the fly's are; the authored `pounce` node sits
  visibly cool between the LC11 cluster and the descending neurons.
- Dragline as a single line; bugs as three-point sprites.
- `--snapshot` for the spider checked into `assets/`.
- **Gate:** a thirty-second `--seconds 30` run on the reference machine at the
  30 fps budget, and the README's measured-vs-modelled table updated with the
  authored edge count.

## 7.1 What was built, and what the data said

*Recorded 2026-09-05, the same day, once every phase was green.*

- **The extract.** `etl_chimera.py` → `data/salticid/circuit.json`: 790
  neurons (789 measured + the pounce node), 26,237 edges (26,110 measured +
  127 authored). LC11 came out at 127 cells (66 left / 61 right). The
  Phase 2 gate was reinterpreted on contact with the data: LC11 is a
  *sensory input* population like LC4, driven by transduction, so "in-circuit
  drive onto it" is not the right gate — but it turned out to be 31,460
  synapses anyway, because its lobula inputs are among the strongest partners
  the extract pulls in. Its measured targets inside the extract are all
  `other` (8,545 syn) and other LC11 (262); it does not reach the steering
  DNs, so the head-orient readout is modelled, as §3 anticipated.
- **The fly did not move.** Every change to `lif.rs` (two new populations,
  one new input, three new rates, a latch) is a no-op for the fly: its
  `--simtest --behaviortest` output is byte-identical before and after, and
  its snapshots hash-identical.
- **The chimera's invariants held without tuning** except two seed-fragile
  checks: the post-stimulus silence check needed a 250 ms tail for the
  membrane to discharge, and the walk-duty band was widened to 20–55%
  (measured 38–51% across seeds; the extract's partner mix differs from the
  fly's). The pounce node fires 4–8 ms into sustained small-object drive,
  never under a loom, never at rest, and prey never fires the giant fiber.
- **The spider suite closes the loop for real**: a bug spawned 95 units ahead
  is seen (LC11), stalked, pounced on via the authored node and caught,
  with no scripted step, across every seed tried.
- **The senses are as blind as promised.** `Foreground` is an enum computed
  from `QueryFullProcessImageNameW`'s base name; the `notify` hook is a
  one-word named pipe, verified end to end from a second process, and junk
  is rejected at both ends.

## 8. Open decisions

1. **Reading (A) or (B) in §1.** Assumed (A).
2. **The name.** "DesktopFly" was already wrong with two creatures
   (`PORT_PLAN.md` §8 decision 5); with a spider it is actively misleading.
3. **The pounce provenance.** Authored connective (assumed) versus a purely
   body-side threshold on LC11 rate with no neural node at all. The first is
   more honest about *where* the invention is; the second has less to explain.
4. **The build hook shape.** A CLI subcommand (assumed) versus a tiny HTTP
   endpoint on localhost. The CLI is harder to reach from a browser-based IDE;
   the endpoint is a listening port in a pet.
5. **Do the bugs read as bugs?** Filter #2 applies to prey too. Three drifting
   points is the proposal; anything with legs is not.
6. **Whether a spider EM connectome exists now.** If it does, §2 and §3 are
   rewritten and the creature stops being a chimera.

### Decisions taken 2026-09-05 ("whatever you think best")

1. **Reading (A).** The spider reacts to your coding; it never types.
2. **Name: defer.** The repo and binary stay `desktop-fly` / `desktopfly` until
   creature #3 ships; the tray and brain window show the creature's own name.
   Renaming a public repo is outward-facing and not worth doing twice.
3. **Pounce is an authored neural node.** It gives the brain window one visibly
   invented thing to point at, which is the honesty mechanism working, not a
   cost.
4. **CLI subcommand over a named pipe.** A pet does not get a listening port.
   Browser-based IDEs can shell out; that is their problem, not the spider's.
5. **Bugs are three drifting points.** Nothing with legs.
6. **Verified, no spider connectome.** Searched 2026-09-05: the jumping spider
   brain (*Marpissa muscosa*) has been mapped by histology, immunohistochemistry
   and microCT only (Steinhoff et al., *Arthropod Structure & Development*
   2017); no synapse-resolution EM dataset exists for any spider. The chimera
   framing stands. LC11 is confirmed as a real small-object detector
   population of roughly 50 cells per hemisphere (Keleş & Frye, *Current
   Biology* 2017), which is enough for a network-driven pathway if its
   in-circuit drive checks out in Phase 2.

## 9. Risks

| risk | severity | mitigation |
|---|---|---|
| The chimera reads as a fake spider brain despite the labels | **High** — it is the project's thesis | §2 rules are tests, not prose; the brain window's provenance colouring is on by default and not switchable off |
| LC11 in-circuit drive is too small and pursuit is noise | High | Phase 2 gate; fall back to a fully authored, labelled pursuit rather than tuning noise into a story |
| Spider fails the creep filter for this user even as glass | High | Render a static glass salticid in Phase 0's snapshot path **before** Phases 2–5; it is one mesh |
| Shell generalisation regresses the fly | Medium | Phase 0 gate is a pixel diff plus both suites |
| Foreground-class sense drifts toward reading titles | Medium | The platform crate exposes an enum, never a string, so there is nothing to leak |
| Eight-leg gait costs frame budget at 30 fps | Low | One phase scalar, eight offsets; the fly's six cost nothing measurable |
