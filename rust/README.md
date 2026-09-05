# DesktopFly — Rust port

The cross-platform rewrite described in [`../PORT_PLAN.md`](../PORT_PLAN.md).
The Swift app at the repo root is unchanged and remains the reference
implementation — it is the oracle this port is checked against.

## Layout

| crate | what it is | OS-coupled? |
|---|---|---|
| `core/` (`dfcore`) | connectome loading, the 1 kHz LIF sim, rates→drives, body behaviour, habituation, both ground-truth suites | **no** — builds and tests on any target |
| `platform/` (`dfplatform`) | the only crate that knows what an OS is; fills in one `EnvSnapshot` 30×/s | yes, per-OS |
| `shell/` (`dfshell`) | overlay window, wgpu renderer, tray, transduction, persistence | yes |
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
```

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
target/release/winprobe.exe --terrain      # what the fly can walk on right now
target/release/senses.exe                  # one second of live desktop senses
```

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

`%LOCALAPPDATA%\DesktopFly\habituation.json` — what the creature has learned
about you. Delete it, or use the tray's "Forget Me", to get a naive creature.
Nothing else is persisted.
