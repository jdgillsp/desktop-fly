# DesktopFly — Rust port

The cross-platform rewrite described in [`../PORT_PLAN.md`](../PORT_PLAN.md).
The Swift app at the repo root is unchanged and remains the reference
implementation — it is the oracle this port is checked against.

## Layout

| crate | what it is | OS-coupled? |
|---|---|---|
| `core/` (`dfcore`) | connectome loading, the 1 kHz LIF sim, rates→drives, body behaviour, habituation, both ground-truth suites | **no** — builds and tests on any target |
| `platform/` (`dfplatform`) | the only crate that knows what an OS is; fills in one `EnvSnapshot` 30×/s | yes, per-OS |
| `shell/` (`dfshell`) | overlay window, wgpu renderer, tray, persistence, and one `Runtime` per creature (`runtime.rs`: brain + body + senses + geometry behind one seam, so `main.rs` never names a species) | yes |
| `spike0/` | the Spike 0 overlay proof — kept because it is the minimal reproducer for transparency problems | yes |
| `winprobe/` | window-terrain / ex-style auditor, used to verify the overlay at OS level | yes |

The important boundary is that **the simulation never touches the OS**. It
consumes six scalars and emits population rates; everything platform-specific
sits upstream of those, behind `dfcore::env::Senses`.

## Build and run

```sh
cargo build --release
target/release/desktopfly.exe              # the fly, on your desktop
target/release/desktopfly.exe --seconds 20 # ...for 20 s, then quit
target/release/desktopfly.exe --snapshot fly.png   # offscreen render
target/release/desktopfly.exe --diag       # per-stage frame tracing
target/release/desktopfly.exe --fps 60     # default is 30; see below
target/release/desktopfly.exe --literal    # photoreal fly instead of glass
target/release/desktopfly.exe --no-brain   # overlay only
target/release/desktopfly.exe --creature c_elegans   # the worm (see below)
```

**Creatures.** The tray has a *Creature* submenu; the choice is saved and
survives a restart, and `--creature ID` overrides it for one run (ids:
`drosophila`, `salticid`, `c_elegans`). Switching keeps the new animal where
the old one was and rebuilds the brain window for its data. The worm's
connectome is **not shipped** (its licence is unverified — `PORT_PLAN.md`
§8), so until `etl_celegans.py` has been run it crawls *brainless* at a
constant drive and the tray says so; nothing about it reacts to you. An
unknown id falls back to the fly with a message rather than a panic.

**Glass or literal.** The tray's *Glass Anatomy* item toggles between the
glass register (the circuit visible inside a translucent body) and the
literal animal, for whichever creature is running; the choice is saved.
`--literal` forces the literal look for one run. The spider's literal
register is a bold jumping spider, *Phidippus audax*: black, three white
abdominal spots, pale leg bands, green chelicerae, big glossy eyes.

**The spider** (`salticid`) is the chimera described in `../SPIDER_PLAN.md`:
the fly's measured modules plus LC11, one authored pounce node, a
jumping-spider body, and two content-blind coding senses. Its data ships in
`../data/salticid/`. The build hook is the whole integration:

```sh
target/release/desktopfly.exe notify fail   # bugs appear; the spider hunts them
target/release/desktopfly.exe notify pass   # the bugs leave
```

Put one of those at the end of a build script, a test task or a git hook.
It travels over a local named pipe (`\\.\pipe\desktopfly-notify`), never a
socket, and carries one word. The other coding sense is the *class* of the
foreground app (editor / terminal / browser / other), from the process name
only — never a title — which makes the spider sit tighter and look around
more while you work. The brain window colours the authored node cool and
draws it larger; nothing invented is allowed to look measured.

**The web builders** (`araneus`, `parasteatoda`, `agelenopsis`) are the
chimeras of `../WEB_PLAN.md`: one shared circuit in `../data/weaver/` (the
fly's modules minus the wings, no LC11, and one authored neuron with no
synapses — a vibration sense), one body chassis (`core/src/weaver.rs`) on
the eight-leg rig the salticid also uses (`core/src/arachnid.rs`), and three
construction programs (`orb.rs`, `cobweb.rs`, `funnel.rs`) that lay their
webs as the path the spider walks, on the silk model in `core/src/silk.rs`.
The programs are procedural — no neurons — and every label says so. They
start in a vivarium; on the open desktop the web hangs under a window edge
and is cut when that window moves. One runtime serves all three
(`shell/src/weaverrt.rs`); the looks differ in `shell/src/weaverbody.rs`.

**Frame rate.** The default is 30 fps, not 60. Spike 0 measured ~8% of a core
just to clear and present a full-screen overlay, so frame rate is a real part
of a background pet's idle cost. Measured here: 60 fps costs 15% of one core,
30 fps costs 11% — a ~27% saving, smaller than it sounds because the 1 kHz
simulation and the 30 Hz sense poll run at fixed rates regardless. A walking
fly reads fine at 30.

Quit from the tray icon. Windows files new tray icons under the overflow
chevron — pin it to see the fly.

## Verify

These are the ground truth, exactly as in `CLAUDE.md` for the Swift build.
**Run both after any change to the sim or behaviour.**

```sh
cargo test --workspace                     # everything, including both suites
target/release/dfcore.exe --simtest        # circuit invariants
target/release/dfcore.exe --behaviortest   # 17 end-to-end sim -> body checks
target/release/dfcore.exe --simtest --seed 1337   # any seed must pass
target/release/dfcore.exe --creature salticid --simtest       # the chimera's 14 circuit checks
target/release/dfcore.exe --creature salticid --behaviortest  # 15 sim -> spider body checks
target/release/dfcore.exe --creature araneus --simtest --behaviortest       # the web builders (also parasteatoda, agelenopsis)
target/release/winprobe.exe --terrain      # what the fly can walk on right now
target/release/senses.exe                  # one second of live desktop senses
target/release/desktopfly.exe --creature salticid --snapshot s.png --zoom 5   # a close look
```

The fly's numbers are the oracle and must not move: after any change to the
simulator, diff `dfcore --simtest --behaviortest` output against the previous
build (it is byte-identical across every change made for the chimera), and
diff `--snapshot` PNG hashes — with `--creature drosophila` named explicitly,
because a bare `--snapshot` follows the tray's saved creature choice. The
chimera's suites guard its own claims.

Current results on the reference machine: GF silent over 4 s of rest, GF fires
**4 ms** after an abrupt loom, walk-drive duty 35–45%, siesta 23–37%, all 17
behaviour checks pass, across every seed tried.

## Deliberate deviations from the Swift build

1. **The PRNG is seeded** (`rng::Pcg32`). Swift used
   `SystemRandomNumberGenerator`, so its suites gave slightly different numbers
   every run and per-neuron baselines were redrawn each launch. A ground truth
   that changes run to run is a weaker one. A seed-fragility test guards against
   this hiding a real failure.
2. **The body emits a `Pose` instead of writing into scene nodes.** Behaviour
   and rendering were entangled in `FlyModel.swift`; separating them is what
   lets a second creature implement `Body` later.
3. **Habituation** (`core/src/habituation.rs`) is new — see PORT_PLAN.md §6.3.
   It is a modelling choice, not measured data, and is labelled as such.
4. **The overlay yields to fullscreen apps** (`SHQueryUserNotificationState`).
   macOS's `.fullScreenAuxiliary` made this unnecessary there.
5. **Provenance is per neuron and per edge** (`Origin`), and `LifSim` carries
   two populations the fly does not use (LC11, pounce) plus a small-object
   input. For the fly these are empty and zero; its suite output is
   byte-identical before and after.
6. **A third creature, the chimera.** See above.

## Platform fidelity

Two macOS senses are genuinely weaker on Windows, and
`Senses::fidelity_notes()` says so at runtime rather than burying it:

- **Typing detection is inferred.** `GetLastInputInfo` returns one combined
  keyboard+mouse timestamp; the APIs that separate them also reveal *which key*,
  which would break the project's "knows when, never what" claim. Inferred as
  recent input **and** a stationary cursor.
- **"Temperature" is machine load.** Windows has no `ProcessInfo.thermalState`;
  derived from processor clock throttling blended with CPU load.

## State on disk

`%LOCALAPPDATA%\DesktopFly\habituation.json` — what the fly has learned about
you; every other creature keeps its own file beside it
(`habituation-c_elegans.json`), because what the fly learned about your cursor
is not what the worm learned about your clicks. Delete one, or use the tray's
"Forget Me", to get a naive creature. `settings.json` in the same folder holds
the creature choice. Nothing else is persisted.
