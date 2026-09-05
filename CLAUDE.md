# DesktopFly — agent notes

A 3D fruit fly on a transparent desktop overlay, behavior-driven by a 1 kHz
leaky-integrate-and-fire (LIF) simulation of a 668-neuron circuit extracted
from the real FlyWire connectome (FAFB v783). The body is procedural; the
brain data is real.

## Two builds — read this first

| | macOS (Swift) | Windows (Rust) |
|---|---|---|
| where | repo root, `*.swift` | `rust/` |
| status | **reference implementation**, unchanged | active development |
| build | `./build.sh` | `cd rust && cargo build --release` |
| suites | `./DesktopFly --simtest --behaviortest` | `cargo test --workspace`, or `dfcore.exe --simtest --behaviortest` |

The Swift app is the **oracle**: the Rust port is checked against it
numerically, and its numbers must not drift. If you are changing simulation or
behavior, run *both* suites in whichever tree you touched.

The port's plan, decisions and progress are in `PORT_PLAN.md`; its layout,
deliberate deviations and platform fidelity notes are in `rust/README.md`.
Everything below this section describes the **Swift** build unless it says
otherwise.

### What differs in the Rust build

- Roles are **data**, not string literals: one manifest in
  `rust/core/src/roles.rs`. The eight-step recipe below collapses to four
  steps in two files (ETL, manifest row, readout, test).
- `Sim`, `Body` and `Creature` are traits (`rust/core/src/creature.rs`), so a
  second creature is an implementation rather than a fork. *C. elegans* runs
  on a **graded, non-spiking** integrator (`rust/core/src/graded.rs`) — its
  neurons do not spike, and its data is not shipped because the licence is
  unverified. In the shell, everything per-creature sits behind one `Runtime`
  (`rust/shell/src/runtime.rs`); the tray picks the creature and `--creature`
  overrides it. A third creature is a `Creature` impl in core plus a `Runtime`
  in the shell — see `SPIDER_PLAN.md`.
- The PRNG is seeded, so the suites are reproducible run to run.
- Scene units are **logical**, not physical pixels — the creature keeps a
  constant apparent size on a scaled display.
- Extras with no Swift counterpart: habituation (persisted), glass-anatomy
  rendering (default; `--literal` for the photoreal fly), and yielding the
  overlay to fullscreen apps.

## Files

| file | contents |
|---|---|
| `main.swift` | overlay scene, CLI modes, `SignalBuilder` (rates→commands), `Coordinator` (render-loop hub), `AppDelegate` (menu, timers, display switching) |
| `FlyModel.swift` | procedural fly body + `Fly` behavior (states, gait, flight, ledges, sleep) |
| `Sim.swift` | data loading, `BrainSignals`, `SpikeBus`, `LIFSim` (CSR network, stimulation API) |
| `BrainView.swift` | brain window: point clouds, click-to-stimulate, spike flashes |
| `Environment.swift` | permission-free senses: `WindowSense` (ledges/looms), circadian curve, user idle, thermal tempo |
| `etl.py` | raw Codex dumps → `data/brain_points.json` + `data/circuit.json` |
| `data/` | shipped derived data (CC BY-NC 4.0 — see `data/DATA_LICENSE.md`) |

## Build, run, verify (macOS / Swift)

```sh
./build.sh                     # bare swiftc, -swift-version 5, no Xcode project
./DesktopFly                   # menu-bar 🪰; quit from there
./DesktopFly --simtest         # circuit invariants (MUST pass after sim/etl changes)
./DesktopFly --behaviortest    # 17 end-to-end sim→body checks (MUST pass after behavior changes)
./DesktopFly --snapshot f.png  # offscreen fly render
./DesktopFly --brainshot b.png # offscreen brain render
```

Always run **both** suites after any change; they are the ground truth.
Key invariants: GF silent over 4 s of rest, GF fires ≤ ~10 ms after abrupt
loom, walk-drive duty 20–50%, siesta (scale 0.84) walk-drive > 3%,
no per-frame scale/z snap at landing.

**SourceKit note**: the IDE reports "Cannot find type ..." across files —
false positives. The five .swift files compile as one module via build.sh;
trust the compiler, not single-file diagnostics.

## Threading model

- SceneKit render thread: `Coordinator.renderer(_:updateAtTime:)` steps the
  sim and updates flies. All cross-thread mutation goes through
  `Coordinator.enqueue {}` (lock + pending-actions queue, drained per frame).
- Main thread: timers (mouse 30 Hz, windows 0.7 s), menu actions, global
  click monitor — these only call enqueue/setters.
- Brain window has its own render delegate; spikes cross via `SpikeBus` (locked).
- `LIFSim.stimulate()` is thread-safe (pending list merged at `step()`).

## Neuron → behavior mapping (current)

| role slug | FlyWire types (count) | drives | consumed in |
|---|---|---|---|
| `lc4`, `lplc2` | LC4 (104), LPLC2 (210) | looming input → nervous darting; excite GF | `BrainSignals.nervous` |
| `gf` | DNp01 (2) | escape takeoff (spike = takeoff) | `BrainSignals.escape` |
| `dna01`, `dna02` | DNa01 (2), DNa02 (2) | steering: L−R rate → turn bias (slow-adapted) | `BrainSignals.turnBias` |
| `dnp09` | DNp09 (2) | walk/rest hysteresis + walking speed | `BrainSignals.walkDrive` |
| `dng11` | DNg11 (6) | grooming hysteresis | `BrainSignals.groomDrive` |
| `mdn` | MDN (4) | backward walking burst | `BrainSignals.backward` |
| `escw` | DNp02/DNp04/DNp11 (6) | wing-beat effort in flight, threat wing-raise | `BrainSignals.wingDrive` |
| `other`+ascending (27) | strongest ascending partners | body→brain gait proprioception (input target) | `sim.gaitDrive/gaitPhase` |
| `other`+sensory (16) | strongest sensory partners | wind/tap input; electrically boosted onto GF | `sim.airPuff`, taps |

Whole-population rate → `BrainSignals.arousal` (spontaneous-takeoff gate,
flight effort). Only fly #1 has the brain; extra flies use legacy
distance-based behavior (`signals: nil` path).

## Adding a new neuron population (recipe)

1. **Check the type exists** in v783:
   `gzcat consolidated_cell_types.csv.gz | grep -c ',TYPE,'` (raw dumps: see
   README "Regenerating the data" for the GCS URLs; don't commit raw dumps).
2. **etl.py**: add `"TYPE": "roleslug"` to `CORE_TYPES`; add the slug to the
   reserved-partner loop AND the in-degree report loop.
3. **Rerun ETL** and read the report: `in-circuit drive onto roleslug` should
   be ≥ several hundred synapses — if it's tiny, the population will be
   noise-driven, not network-driven (this bug shipped once for DNg11: 6 syn).
4. **Sim.swift**: group array (`private(set) var xyz: [Int]`), populate in the
   init role switch, baseline (command DNs: deterministic `0.036`; never
   random per-side for bilateral pairs — asymmetry must come from wiring),
   rate EMA (`rateXyz`) in the spike-counting switch.
5. **SignalBuilder** (main.swift): normalize `rateXyz` into a new
   `BrainSignals` field — **always clamp** (an unclamped walkDrive once sent
   the fly to 1,100 pt/s).
6. **FlyModel.brainBehavior**: consume the signal. Use hysteresis + the
   `stateAge` dwell guard (≥0.4 s) for state changes, cooldown timers for
   one-shot actions; make sure the action works from every grounded state
   (MDN was once dead from idle).
7. **BrainView.swift**: role color in the circuit overlay + `regionName` label
   (clicking that region should demo the behavior).
8. **Tests**: add a `--behaviortest` scenario (stimulate population → assert
   body reaction) and, if sim-level, a `--simtest` probe. Run both suites.

## Tuning gotchas (learned the hard way)

- **Operating point is razor-thin**: neurons rest at `baseline × 20.4` vs
  threshold 1.0 (tau 20 ms). Never scale baselines linearly by a mood/time
  factor — compress toward 1 (`1 − (1−a)×0.35`), or populations go silent
  (the "siesta coma" bug).
- **Escape is a race**: LC→GF electrical drive (×6 boost) vs ~1,200 syn of
  feedforward inhibition (4 ms delayed). Slow ramps lose to inhibition by
  design — test escapes with **abrupt** loom steps, not ramps.
- **Live modifiers must never weaken takeoff**: flight effort =
  `max(baseEffort, live formula)` (a regression once halved escape altitude).
- Weight scale 0.0008/synapse; refractory 2 ms; inhibitory synaptic delay
  4 ms (ring buffer); `weightScale`/`gapJunctionBoost` live in `Sim.swift`.
- Landing must go through the flare (alt decays below 0.035) — never snap
  scale/z in `land()`.

## Repo conventions

- Public repo: `DenisSergeevitch/desktop-fly` (master). Code MIT; `data/` is
  CC BY-NC 4.0 (FlyWire terms) — keep the license split intact.
- README numeric claims (neuron/edge/synapse counts, latencies) must match
  `data/*.json` and suite output — reviewers falsify them against the data.
- `.gitignore` covers the binary, logs, and root-level PNGs (diagnostics
  outputs); intentional images live in `assets/`.
- Local folder is `fly-brain`; the remote is `desktop-fly` — harmless.
