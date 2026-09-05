# DesktopFly → Windows + a second creature

**Status:** assessment & architecture only. No port code written. For review.
**Branch:** `windows-port-plan`
**Date:** 2026-08-23
**Scope:** (1) how to get this onto Windows, (2) what is portable vs. what must be
rewritten per-OS, (3) a creature/organism abstraction so the engine is not
hardcoded to *Drosophila*, (4) a phased plan.

---

## 0. TL;DR

**Recommended approach: a cross-platform Rust core + a thin Rust shell
(`winit` + `wgpu` + `tray-icon`), with all OS senses behind one trait
implemented per-platform (`windows-rs` on Windows, `objc2` on macOS).**

The reasoning in one paragraph: the precious part of this repo is ~900 lines of
data + simulation + behavior math that has no OS dependency at all. Everything
else — the overlay window, the tray item, the scene graph, the window-terrain
sensing — is replaceable plumbing that Apple happened to give you for free and
Windows does not. Every credible option therefore involves rewriting the render
and platform layers; the only real question is *what you rewrite them in*. Rust
keeps the "one binary, `cargo build`, no project file" spirit of the current
`swiftc *.swift` build, keeps macOS alive from the same source, and — verified
below — now has a working transparent-GPU-overlay path on Windows that it did
not have a year ago.

**Recommended second creature: *C. elegans*, rendered stylised or diagrammatic**
(302 neurons, the only complete cell-identified whole-animal connectome). Two
things to know before signing off:

- **It needs a different integrator.** *C. elegans* neurons are mostly
  **non-spiking** — an honest worm needs a *graded* membrane-potential model with
  explicit gap junctions, not the LIF spiking integrator the fly uses. "Just swap
  the data file" would produce a fake worm. §5 designs the abstraction to carry both.
- **Every complete connectome belongs to an invertebrate**, so the "don't creep
  me out" constraint can't be solved by picking a cuddlier animal — there isn't
  one with the data. It has to be solved in the rendering, which §6.1 argues is
  the better lever anyway, and which opens up a genuinely novel option: a pet
  whose body *is* its own nervous system.

**Decisions** (details in §8):

1. ~~**Shell**~~ — **DECIDED 2026-08-23: Rust, gated on Spike 0.** Godot 4 stays
   the documented fallback if the spike fails; `core/` is unaffected either way.
2. **Second creature** — narrowed 2026-08-23 by a constraint that wasn't in the
   first draft: *it must not be creepy to look at all day.* That kills the
   *Drosophila* larva (it's a maggot) and promotes two options. **§6.1 argues
   the constraint is mostly an art-direction decision, not a species one.**
3. **macOS**: does the mac build stay alive on the new core (recommended), or does this fork go Windows-only and the mac Swift app is frozen?
4. **Rendering register** (new, from §6.1): literal / stylised / diagrammatic.
   This now gates the creature choice, so it wants answering first.

Rough sizing, solo and focused: **5–8 weeks** to a Windows build with both
creatures and macOS parity. A walking fly on Windows with no brain window is
**~2–3 weeks**.

---

## 1. What the app actually is

2,372 lines of Swift in five files, plus a Python ETL and two derived JSON files.
It cleanly separates into four layers — which is why this port is tractable at
all — but the layers are not currently expressed as modules, only as habits.

| Layer | Where it lives now | OS-coupled? |
|---|---|---|
| **Data** | `data/circuit.json` (668 neurons / ~19k signed edges), `data/brain_points.json` (23,210 somas), `etl.py` | No |
| **Sim core** | `Sim.swift` — CSR adjacency, 1 kHz LIF loop, delayed inhibition ring buffer, population rate EMAs, stimulation API | No (only `Foundation` + `simd`) |
| **Behavior mapping** | `SignalBuilder` (`main.swift:463`), `Fly.brainBehavior` (`FlyModel.swift:445`), gait/flight kinematics (`FlyModel.swift:509-680`) | No (math only; applies results to SceneKit nodes) |
| **Sensory transduction** | `computeLoom` (`main.swift:606`), `injectTap` (`main.swift:592`), `injectWindowLoom` (`main.swift:579`), `circadianActivity` (`Environment.swift:72`) | No (consumes cursor/window/time values) |
| **Render** | SceneKit throughout: `buildScene` (`main.swift:17`), `buildFlyModel` (`FlyModel.swift:144`), `BrainView.swift` entire | **Yes — Apple-only** |
| **Platform / senses** | `AppDelegate` (`main.swift:693`), `WindowSense` (`Environment.swift:16`), `userIdleSeconds`, `thermalTempo` | **Yes — Apple-only** |

The single most important structural observation for the port:

> **The sim never touches the OS.** It consumes six scalars (`loomL`, `loomR`,
> `gaitDrive`, `gaitPhase`, `airPuff`, `activityScale`/`sensoryGate`) and emits
> seven rates. Everything platform-specific is upstream of those six numbers.

That means the platform boundary can be a single plain-data struct — call it
`EnvSnapshot` — filled by the OS layer and consumed by transduction. Get that
seam right and the Windows work never contaminates the science.

---

## 2. Windows port: the options

### The crux risk, stated first

Everything hinges on one capability: **a hardware-accelerated, per-pixel-alpha,
click-through, always-on-top overlay window on Windows.** macOS gives this away
in six lines (`main.swift:733-740`: borderless + `.clear` background +
`isOpaque = false` + `ignoresMouseEvents` + `.floating` + `canJoinAllSpaces`).
Windows does not.

The naive Windows route — `WS_EX_LAYERED` + `UpdateLayeredWindow` — is a CPU
blit of the whole window every frame. It works, and it will feel like it works,
until you notice you are burning a core to move a fly at 120 fps. The two real
routes are:

- **`DwmEnableBlurBehindWindow` with a NULL blur region**, which enables
  per-pixel alpha on a normally-composited window. Cheap, widely used, but it is
  a side-effect of an API meant for something else and has historically been
  flaky across GPU drivers and Windows versions.
- **DirectComposition**: `WS_EX_NOREDIRECTIONBITMAP` + `DCompositionCreateDevice`
  + `CreateSwapChainForComposition` with premultiplied alpha. This is the
  supported, correct mechanism. It is what modern transparent overlays use.

**Verified, and it changes the recommendation:** `wgpu` now ships the
DirectComposition path directly —
`Dx12BackendOptions { presentation_system: Dx12SwapchainKind::DxgiFromVisual }`
creates the DXGI swapchain from an auto-managed `IDCompositionVisual` over the
window's HWND, and its documentation states plainly that this option *supports
transparent windows* (added ~wgpu 27; before that, DX12 transparency was
[broken](https://github.com/gfx-rs/wgpu/issues/7108) and this was the strongest
argument against the Rust path). You can also hand wgpu your own visual via
`SurfaceTargetUnsafe::CompositionVisual` if you want to own the composition tree.

This is still **Spike 0** in §7 — verified-in-docs is not verified-on-your-GPU,
and the spike must prove the whole combination (transparent + click-through +
no taskbar entry + always-on-top + multi-monitor + 120 fps), not just alpha.

### (a) Swift on Windows

Swift *does* run on Windows: official toolchain, MSVC integration, working
Foundation (`swift-corelibs-foundation` provides `JSONDecoder`, `NSLock`,
`CGFloat`/`CGPoint`/`CGRect`), and a `WinSDK` module that exposes Win32 directly.
`Sim.swift` would port with almost no edits — you'd shim the handful of
`simd_*` free functions (`simd_normalize`, `simd_distance`, `simd_dot`), since
`SIMD3<Float>` itself is stdlib.

And then you hit the wall: **AppKit and SceneKit do not exist on Windows, and
nothing replaces them.** There is no Swift 3D engine, no Swift scene graph, no
maintained Swift GUI toolkit for Windows (`swift-win32` is a community Win32
wrapper, not a renderer). Direct3D is COM, and Swift's COM interop is the worst
of any systems language in this list. You would be writing a D3D11/12 renderer
through Swift's C interop as the first person to seriously try it.

**Verdict: reject.** It buys you ~350 portable lines you get almost as cheaply
by transliterating, and charges you the entire renderer in the ecosystem with the
least support for building one.

### (b) Cross-platform rewrite — **recommended**

The core is transliterated once; the shell is written once per surface.

**Rust — `winit` + `wgpu` + `tray-icon` + `windows-rs`/`objc2`** *(recommended)*

- Sim port is mechanical: flat arrays, CSR, f32 math, a ring buffer. Rust will
  also be meaningfully faster than the current Swift loop, which matters if you
  ever want more than 668 neurons (a *Drosophila* larva at 3,016 would be fine;
  a graded worm at 302 is free).
- Overlay: `winit` gives `with_transparent`, `with_decorations(false)`,
  `WindowLevel::AlwaysOnTop`, `with_skip_taskbar`, and — critically —
  `set_cursor_hittest(false)`, which is the cross-platform equivalent of
  `ignoresMouseEvents`. Combined with the wgpu DirectComposition path above,
  the crux is addressed on both OSes from one code path.
- Tray: `tray-icon` covers Windows (Shell_NotifyIcon), macOS (NSStatusItem) and
  Linux from one API.
- Senses: `windows-rs` is a first-class, Microsoft-published binding — every API
  in §4 is directly callable.
- **Cost:** you write the renderer. SceneKit is doing more for this app than it
  looks: primitive meshes (sphere/capsule/cone), a transform hierarchy, a
  Blinn-Phong material model, a shadow-casting directional light, an orthographic
  camera, an extruded-Bézier wing, a point-cloud geometry with screen-space point
  sizing and additive blending, `unprojectPoint` for click-picking, and offscreen
  snapshot rendering. Estimate **1,500–2,500 lines** to replace, one time.
  `three-d` (a mid-level scene layer over wgpu/glow with primitives, lights and
  shadow maps) can cut that substantially if the renderer starts dragging —
  keep it in your pocket rather than adopting it up front.

**Godot 4 — the low-risk alternative**

Godot erases nearly all of that cost: scene graph, primitive meshes, shadowed
directional light, orthographic camera, `ArrayMesh` with `PRIMITIVE_POINTS` for
the brain cloud, ray-picking, and — verified — `FLAG_TRANSPARENT`,
`FLAG_ALWAYS_ON_TOP`, `FLAG_BORDERLESS`, `window_set_mouse_passthrough()` for
click-through, plus `DisplayServer.create_status_indicator()` for a tray icon
(**implemented on Windows and macOS**; not Linux). Desktop pets are a
well-trodden Godot use case, so the crux is de-risked by other people's shipped
software rather than by your spike.

Costs: a ~70 MB export instead of a ~5 MB binary; an engine + editor +
export-template dependency; a project file where there is currently a two-line
shell script; and the sim behind a language boundary (C#, or Rust via `gdext`
GDExtension — GDScript is the wrong tool for a 1 kHz inner loop). Window-terrain
sensing still needs native calls either way.

*If Spike 0 fails or drags past ~3 days, this is the fallback — and because the
core is a separate crate, switching shells costs you the shell only.*

**Electron/Tauri + Three.js** — fastest to a demo, genuinely good for the brain
window (HTML labels over WebGL is nicer than what SceneKit does today), and both
support transparent + click-through windows. But it's a browser on your desktop
for a 5 mm fly, the sim wants WASM to be honest about the 1 kHz loop, and window
enumeration needs native shims regardless. Reject on footprint and spirit;
reconsider only if a web-shareable version becomes a goal.

**C++/Qt** — Qt gives `WA_TranslucentBackground` + `Qt::WindowTransparentForInput`
and a real widget/3D story, but the licensing question, the build weight, and
C++'s ergonomics versus Rust for a from-scratch port make it strictly dominated
here. Reject.

### (c) Native Windows: .NET/WinUI or Win32 + Direct3D

Best possible fidelity and the smallest impedance mismatch with the Windows APIs
in §4 — you'd be calling `EnumWindows` and `DwmGetWindowAttribute` without a
binding layer, and DirectComposition is right there.

But: **WPF's `AllowsTransparency=true` drops the window to software rendering**,
which is exactly the trap this app must avoid, and **WinUI 3 has no good story
for a transparent click-through overlay** and no scene graph at all. So the real
native option is C# + Silk.NET/Veldrid + hand-rolled Win32 overlay, or
C++/Win32 + D3D11 + DComp — i.e. you write the renderer *and* abandon macOS,
which is the one thing option (b) gets you for free.

**Verdict: reject unless you decide this fork is Windows-only forever** (decision
#3 in §8). If you do decide that, C# + Silk.NET is the version of this I'd pick.

### Recommendation

```
desktop-critter/
├─ core/            # Rust lib, zero OS deps: connectome, sim, drives, behavior, body math
│  └─ creatures/    #   drosophila/  c_elegans/          (see §5)
├─ shell-desktop/   # winit + wgpu + tray-icon; renderer; the app loop
├─ platform/        # trait Senses; windows.rs (windows-rs) | macos.rs (objc2)
├─ data/            # unchanged JSON + DATA_LICENSE.md; per-creature subdirs
└─ etl/             # etl.py unchanged + a second ETL per creature
```

`core/` is testable headless — which matters, because per `CLAUDE.md` the two
test suites are the ground truth and they must survive the port intact.

---

## 3. Portable as-is

Everything here is a transliteration, not a redesign. Behaviour must be
bit-comparable enough that `--simtest` and `--behaviortest` still pass with the
same numbers.

| What | Where | Notes for the port |
|---|---|---|
| Connectome data | `data/circuit.json`, `data/brain_points.json` | **Unchanged.** Keep the MIT-code / CC BY-NC-data license split intact — it is a FlyWire term, not a preference. |
| ETL | `etl.py` | 100% portable. One mac-ism to fix: `CLAUDE.md` says `gzcat`; use `gzip -cd` on Windows/Git Bash. |
| LIF integrator | `Sim.swift:243` `step()` | Mechanical. Constants must be copied exactly: decay `0.9512` (20 ms τ), threshold `1.0`, refractory `2 ms`, `weightScale 0.0008`, `pNoise 0.0022`, `noiseKick 0.42`, `loomGain 0.30`, `rateAlpha 1/120`, `inhDelayMs 4`, `gapJunctionBoost 6.0`. `CLAUDE.md` warns the operating point is razor-thin — treat these as data, not tunables. |
| CSR construction + gap-junction boost | `Sim.swift:163` | Mechanical. |
| Delayed-inhibition ring buffer | `Sim.swift` `inhQueue` | Mechanical. This is what makes escape a race; don't "simplify" it. |
| Per-role baselines | `Sim.swift:163` init switch | Note the deliberate asymmetry: bilateral command DNs get a *deterministic* `0.036`, only `other` neurons get randomised baselines. Preserve that. |
| Population rate EMAs | `Sim.swift:243` | Mechanical. |
| Stimulation API | `Sim.swift:155` | Mechanical; the lock becomes a `Mutex`/channel. |
| Rates → drives | `SignalBuilder.make`, `main.swift:463` | Includes the 8 s DNa adaptation that keeps steady walking straight. Port verbatim. |
| Behaviour state machine | `Fly.brainBehavior`, `FlyModel.swift:445` | Thresholds, hysteresis, the ≥0.4 s dwell guard, cooldowns. Port verbatim. |
| Gait / flight kinematics | `FlyModel.swift:509-680` | Pure math. Only the final "write into a scene node" lines are renderer-specific. |
| Cursor → loom transduction | `computeLoom`, `main.swift:606` | Pure math over cursor position + dt. |
| Circadian curve | `Environment.swift:72` | Pure function of hour. |
| Both test suites | `main.swift:125` `runSimtest`, `main.swift:231` `runBehaviorTest` | **Port these first, before any rendering.** They are the only thing that will tell you the transliteration is faithful. |

**Port order matters:** core + tests → green suites → *then* pixels. If you
render first you will spend a week wondering whether a bug is in your LIF loop or
your shadow matrix.

---

## 4. Must be reimplemented — and the Windows equivalents

This is the actual work list for the platform layer. Behind the trait, all of it
collapses to "fill in an `EnvSnapshot` 30 times a second."

| Capability | macOS today | Windows equivalent | Fidelity |
|---|---|---|---|
| **Overlay window** | borderless `NSWindow`, `isOpaque=false`, `.clear`, `.floating`, `ignoresMouseEvents` (`main.swift:733`) | `WS_EX_LAYERED \| WS_EX_TRANSPARENT \| WS_EX_TOOLWINDOW \| WS_EX_NOACTIVATE`, per-pixel alpha via DirectComposition (`WS_EX_NOREDIRECTIONBITMAP` + composition swapchain) or `DwmEnableBlurBehindWindow`; `HWND_TOPMOST` | ✅ with DComp |
| **Click-through** | `ignoresMouseEvents = true` | `WS_EX_TRANSPARENT` (winit: `set_cursor_hittest(false)`) | ✅ |
| **No dock/taskbar entry** | `setActivationPolicy(.accessory)` | `WS_EX_TOOLWINDOW` | ✅ |
| **All Spaces / virtual desktops** | `.canJoinAllSpaces` (`main.swift:740`) | **No supported equivalent.** `IVirtualDesktopManager` is partly undocumented and breaks across builds. Realistically the fly lives on the desktop it was created on. | ⚠️ **degraded** |
| **Menu-bar item** | `NSStatusItem` + `NSMenu`, emoji title (`main.swift:829`) | `Shell_NotifyIcon` + `TrackPopupMenu` on `WM_RBUTTONUP` (or the `tray-icon` crate). Needs a real `.ico` — no emoji-as-icon. | ✅ (needs art) |
| **Window terrain (ledges)** | `CGWindowListCopyWindowInfo`, filter `layer==0`, own PID, `alpha>0.05`, min size (`Environment.swift:26`) | `EnumWindows` + `IsWindowVisible` + `GetWindowLong(GWL_EXSTYLE)` to drop `WS_EX_TOOLWINDOW` + `GetWindowTextLength()>0` + **`DwmGetWindowAttribute(DWMWA_CLOAKED)`** to skip cloaked UWP/background windows + `GetWindowThreadProcessId` to exclude self | ✅ but fiddlier |
| **True window bounds** | `kCGWindowBounds` is visually correct | **`GetWindowRect` is wrong** — it includes invisible resize borders, so the fly would walk in mid-air a few px off the title bar. Use `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`. | ✅ if you use DWM |
| **Window identity** | `kCGWindowNumber` → `Ledge.id` | `HWND` (stable while alive; may be recycled — pair with the PID or a generation counter) | ✅ |
| **New-window looms** | diff of window-ID set (`Environment.swift`) | same diff over `HWND` set; or `SetWinEventHook(EVENT_OBJECT_SHOW)` for event-driven | ✅ |
| **Cursor position** | `NSEvent.mouseLocation` | `GetCursorPos` | ✅ |
| **Clicks as substrate taps** | `NSEvent.addGlobalMonitorForEvents` — permission-free (`main.swift:795`) | Two options: `SetWindowsHookEx(WH_MOUSE_LL)` (works, no prompt, but global hooks are an EDR/antivirus heuristic and this app should not look like a keylogger) **or** poll `GetAsyncKeyState(VK_LBUTTON)` at 30 Hz — zero privacy surface, occasionally misses a very fast click. **Recommend polling.** | ✅ |
| **Typing = vibration** | `CGEventSource.secondsSinceLastEventType(.keyDown)` — *when*, never *what* (`main.swift:770`) | **`GetLastInputInfo` returns one combined timestamp** and cannot separate keyboard from mouse. The APIs that can (`WH_KEYBOARD_LL`, raw input) also reveal *which key*, which breaks the project's privacy claim. **Recommend the honest heuristic:** recent input **AND** cursor position unchanged ⇒ typing. | ⚠️ **inferred, not measured** |
| **User idle** | `CGEventSource` min across event types (`Environment.swift:84`) | `GetLastInputInfo` (combined) — fine, this is exactly what it's for | ✅ |
| **Thermal tempo** | `ProcessInfo.thermalState`, 4 levels (`Environment.swift:91`) | **No equivalent.** WMI `MSAcpi_ThermalZoneTemperature` needs admin and is often unimplemented. **Recommend a proxy:** `CallNtPowerInformation(ProcessorInformation)` → `CurrentMhz/MaxMhz` (clock throttling) blended with `GetSystemTimes` CPU load. | ⚠️ **"busy machine", not "hot machine"** |
| **Multi-monitor** | `NSScreen.screens`, `didChangeScreenParametersNotification` (`main.swift:806`) | `EnumDisplayMonitors` + `GetMonitorInfo`; react to `WM_DISPLAYCHANGE` | ✅ |
| **DPI** | points; Retina is transparent to you | **Per-monitor DPI is a real landmine.** Call `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`, handle `WM_DPICHANGED`, and decide once whether the fly is sized in physical pixels (shrinks on a 4K panel) or logical (constant apparent size — **recommended**). | ⚠️ **needs a decision** |
| **Coordinate system** | Cocoa: origin bottom-left of primary, **y up**; the code already converts (`Environment.swift:30-52`) | Win32: origin top-left of primary, **y down**, and the virtual screen can have **negative** coordinates for monitors left of / above the primary. | ⚠️ **highest bug density** — write a `ScreenSpace` type with explicit conversions and unit tests, don't sprinkle `primaryH - y` |
| **Fullscreen apps / games** | `.fullScreenAuxiliary` behaves | Topmost overlays flicker or vanish over exclusive-fullscreen D3D. Query `SHQueryUserNotificationState` for `QUNS_RUNNING_D3D_FULL_SCREEN` / `QUNS_PRESENTATION_MODE` and **hide the fly** — also just good manners during a presentation. | ✅ new behaviour, worth adding |
| **Renderer** | SceneKit (`main.swift:17`, `FlyModel.swift:144`, all of `BrainView.swift`) | wgpu: transform hierarchy, primitive mesh gen (sphere/capsule/cone), Blinn-Phong, one directional shadow map, orthographic camera, point-cloud pipeline with screen-space size + additive blend, ray-unproject picking, offscreen render for `--snapshot`/`--brainshot` | ❌ **full rewrite, ~1.5–2.5k lines** |
| **Threading** | SceneKit render-thread delegate + `Coordinator.enqueue` lock queue + `SpikeBus` (`main.swift:487`) | Same shape, but `winit` is an event loop, not a render delegate: sim on its own thread at a fixed 1 kHz accumulator, render thread pulls the latest state. Cleaner than today, actually. | ✅ |

**Two fidelity losses worth naming to users, not hiding:** typing detection
becomes an inference on Windows, and "temperature" becomes "machine load."
Update the README per-platform — the project's whole credibility rests on being
precise about what is measured versus modelled.

---

## 5. The creature abstraction

### The problem with the current shape

Roles are hardcoded as string literals in at least six places: the group-assignment
switch (`Sim.swift:163`), the baseline switch (same init), the spike-counting
switch (`Sim.swift:243`), `SignalBuilder` (`main.swift:463`), `brainBehavior`
(`FlyModel.swift:445`), and the brain-view colour/label tables (`BrainView.swift`).
`CLAUDE.md`'s own "adding a new neuron population" recipe is **eight steps across
five files** — that is the abstraction asking to be written. A second creature is
not an extension of this design; it is the thing that forces the design.

### The seams

```rust
/// Everything the engine needs to run one species. Chosen once at startup.
pub trait Creature {
    type Drives: DriveSet;                       // species-specific command vocabulary
    type Body: Body<Drives = Self::Drives>;

    fn id(&self) -> &'static str;                // "drosophila" | "c_elegans"
    fn connectome(&self) -> &Connectome;
    fn dynamics(&self) -> DynamicsSpec;          // WHICH integrator, plus its params
    fn transduction(&self) -> Box<dyn Transduction>;
    fn readout(&self) -> Box<dyn Readout<Drives = Self::Drives>>;
    fn body(&self) -> Self::Body;
    fn brain_view(&self) -> BrainViewSpec;       // point cloud, role colours, region names
}
```

The shell holds a `Box<dyn ActiveCreature>` façade that erases the associated
types; the core stays monomorphised and fast.

**Connectome** — note the second edge list, which the fly does not currently need
and the worm absolutely does:

```rust
pub struct Connectome {
    pub neurons:    Vec<Neuron>,        // id, cell_type, role: RoleId, side: Side, pos: Vec3
    pub chemical:   Vec<Edge>,          // pre, post, signed weight (syn count × nt sign)
    pub electrical: Vec<Edge>,          // gap junctions — symmetric, unsigned, bidirectional
    pub groups:     HashMap<RoleId, Vec<usize>>,   // built from DATA, not from a match arm
    pub provenance: Provenance,         // source, version, citation, LICENSE
}
```

Two things this fixes immediately: `groups` is derived from a per-creature **role
manifest** (a small TOML/JSON next to the data) instead of a `switch`, so adding a
population becomes *edit ETL + edit manifest + write the readout + write a test* —
four steps in two files instead of eight across five. And `provenance` makes the
data licence a first-class field rather than a README promise, which matters the
moment a second dataset with different terms arrives.

**Dynamics — the important one:**

```rust
pub enum DynamicsSpec {
    /// Spiking. Drosophila. What Sim.swift does today.
    Lif(LifParams),      // τ, threshold, refractory, weight_scale, noise, inh_delay, gap_boost, baselines

    /// Graded / non-spiking with explicit electrical coupling. C. elegans.
    Graded(GradedParams) // C_m, G_leak, E_leak, g_syn, sigmoid (V_eq, β), g_gap, E_rev per NT
}
```

*Why this is not optional:* the fly's LIF model, and the entire escape-latency
story that makes this project interesting, depends on spikes and on a 4 ms
inhibitory delay racing a gap-junction-boosted excitation. **C. elegans neurons
are predominantly non-spiking** — signalling is by graded potential — and its
electrical (gap-junction) connectome is a separate, roughly comparably important
layer to the chemical one. Running worm data through the fly's LIF loop would
produce spikes that the animal does not have and would quietly turn "the brain
data is real" into a lie. The canonical honest model is the graded conductance
formulation from the *C. elegans* tap-withdrawal literature (Wicks, Roehrig &
Rankin 1996) and its whole-network descendants — a linear RC membrane with
sigmoidal synaptic activation and ohmic gap junctions. It is *simpler* to
implement than the LIF loop, and at 302 neurons it costs nothing.

Consequence for the sim core: `trait Sim { fn step(&mut self, ms: u32); fn stimulate(..); fn readout_state(&self) -> &[f32]; }` with `LifSim` and `GradedSim` as
implementations. `BrainView` reads "activity per neuron" — normalised spike
flashes for the fly, normalised depolarisation for the worm.

**The platform seam:**

```rust
/// The ONLY thing the OS layer produces. Everything below here is portable.
pub struct EnvSnapshot {
    pub cursor: Vec2, pub cursor_vel: Vec2, pub clicks: Vec<Vec2>,
    pub ledges: Vec<Ledge>, pub new_windows: Vec<(Vec2, f32)>, pub closed_windows: Vec<LedgeId>,
    pub idle_secs: f32, pub typing_level: f32,
    pub machine_heat: f32,        // 0..1 — thermalState on mac, clock-throttle proxy on Windows
    pub local_hour: f32, pub display: Rect,
}

pub trait Senses { fn poll(&mut self) -> EnvSnapshot; }   // impl per OS
pub trait Transduction { fn apply(&mut self, env: &EnvSnapshot, body: &BodyState, sim: &mut dyn Sim); }
```

Transduction is deliberately **per-creature, not per-engine** — it is where the
"cursor is a looming predator" choice lives, and a different animal makes a
different choice (§5.2). It is also, per the README's honesty section, the layer
where modelling ends and measurement begins; keeping it in one named trait makes
that boundary documentable.

**Body — and the body→brain loop:**

```rust
pub trait Body {
    type Drives: DriveSet;
    fn build(&self, scene: &mut Scene) -> BodyHandle;     // procedural geometry
    fn step(&mut self, dt: f32, d: &Self::Drives, world: &World) -> BodyState;
    fn substrate(&self) -> Substrate;                     // Walker | Crawler | Flier
}
pub struct BodyState { pub pos: Vec2, pub heading: f32, pub locomotion: f32,
                       pub proprioception: Proprioception, /* ... */ }
```

`Proprioception` must be first-class, not bolted on. The fly already closes the
loop (gait phase → ascending neurons, `main.swift:668`). For the worm it is
*load-bearing*: undulation is generated by proprioceptive coupling along the body
(B-type motor neurons responding to local curvature), so the worm's body is not
a display of the brain's output — it is part of the circuit.

### 5.1 Fly as implementation #1

Almost entirely a re-homing of existing code, and it should produce
byte-identical test results:

| Interface | Fly implementation |
|---|---|
| `connectome` | `data/drosophila/circuit.json` + `brain_points.json`, unchanged |
| role manifest | `lc4, lplc2, gf, dna01, dna02, dnp09, dng11, mdn, escw, ascending, sensory` — lifted verbatim from the `CLAUDE.md` mapping table |
| `dynamics` | `Lif(...)` with today's constants |
| `transduction` | cursor kinematics → bilateral loom (`computeLoom`); clicks → `sens` stimulation; window appearance → loom; typing → air puff; circadian + idle → `activityScale`/`sensoryGate` |
| `Drives` | today's `BrainSignals` exactly: escape, nervous, turnBias, backward, walkDrive, groomDrive, wingDrive, arousal, tempo, sleep |
| `body` | `Substrate::Walker` + flight: tripod gait, wing beat, altitude scaling, ledge latching, grooming, sleep posture |
| `brain_view` | 23,210 somas, super-class palette, click-to-stimulate |

### 5.2 *C. elegans* as implementation #2 (pending your call — §6)

| Interface | Worm implementation |
|---|---|
| `connectome` | 302 neurons (279 with synapses), every one individually named — **chemical *and* gap-junction layers**, both required |
| role manifest | Command interneurons `AVB`/`PVC` (forward), `AVA`/`AVD`/`AVE` (reverse); motor classes `VB/DB` (forward), `VA/DA` (backward), `VD/DD` (inhibitory); touch `ALM/AVM` (anterior), `PLM/PVM` (posterior); `AFD` (thermosensory); `AWA/AWC` (chemosensory); `RIS` (sleep/quiescence); `RIM/RIV/SMD` (omega turns) |
| `dynamics` | `Graded(...)` — see above |
| `transduction` | **The worm is blind — the cursor cannot be a looming predator.** Instead: cursor contact at the *anterior* end → `ALM/AVM` mechanosensation → reversal; contact at the *posterior* end → `PLM` → accelerate forward. This is more apt for a desktop pet than vision: the cursor becomes a physical poke rather than a shadow. |
| | **Clicks → tap.** The canonical *C. elegans* behavioural assay is literally tap-withdrawal, and it **habituates** with repetition — so a worm that stops flinching at your clicking is real behaviour, not a hack. |
| | **Machine heat → thermotaxis.** *C. elegans* migrates toward its cultivation temperature via `AFD`. A worm that drifts toward the cooler part of the screen when your machine is pegged is the single best sense-mapping available in this whole design — and it repurposes the Windows thermal proxy whose fidelity loss §4 flags. |
| | **Window edges → thigmotaxis** (worms track surfaces) — the existing ledge system works unchanged. **Idle/night → quiescence** (`RIS`) — the existing sleep system works unchanged. Optional: cursor dwell = "food" → chemotaxis via `AWC`. |
| `Drives` | `forward`, `reverse`, `omega_turn`, `speed`, `undulation_freq`, `arousal`, `quiescent` |
| `body` | `Substrate::Crawler`: 12–24 segments, a travelling sinusoid whose phase advance is driven by motor-neuron output and *fed back* by curvature proprioception. **No legs, no wings, no flight** — which is why `Body` must be a trait and not a `Fly` with flags. Geometry is far simpler than the fly's: a swept capsule chain. |
| `brain_view` | 302 named points is a *better* brain window than the fly's — you can label individual neurons ("AVA — reverse command") instead of regions, and click-to-stimulate becomes click-a-specific-cell. |

The worm is a smaller sim, a simpler body, a richer brain window, and a sense
mapping that fits the desktop better than the fly's. Its cost is entirely in the
second dynamics engine — which is exactly the cost the abstraction exists to
make payable once.

---

## 6. Second-creature candidates — **DECISION NEEDED**

Two hard filters:

1. **A complete, published, synapse-resolution connectome must exist and be
   redistributable.** Without it the app's central claim collapses. This
   eliminates every charismatic option — cat, octopus, honeybee, ant, tardigrade,
   zebrafish — and leaves a very short list.
2. **It must not be creepy to have on your screen all day** (Jesse, 2026-08-23).
   This is a real product constraint, not a nice-to-have — a desktop pet you
   don't want to look at is a failed desktop pet. It interacts badly with filter
   #1, because the complete-connectome list is *entirely* invertebrates.

| Candidate | Size / completeness | Dynamics needed | Body | Creep factor | Verdict |
|---|---|---|---|---|---|
| ***C. elegans*** hermaphrodite | **302 neurons (279 synaptic), complete, cell-identified** — White et al. 1986; Cook et al. 2019 covers both sexes (385 male) | **Graded + gap junctions** (new engine, ~1 week) | Trivial: segmented sinusoid | **Low *if* styled — see §6.1.** The animal is genuinely transparent and 1 mm long; photoreal = a worm, stylised = a glowing filament | ⭐ **Still recommended, conditional on art direction** |
| ***Drosophila* larva** | **3,016 neurons, ~548k synapses, complete brain** — Winding et al. 2023, *Science* | Reuses the LIF engine (**major saving**) | Peristaltic crawler + head casting | **High.** It is a maggot. No styling saves this | **Rejected on filter #2** despite being the cheapest to build |
| **Adult fly + VNC (MANC)** | Not a second creature — a *deeper* one. FlyWire is brain-only; the ventral nerve cord connectome would make the currently-procedural **tripod gait real** | Same LIF engine | Existing fly body, now neurally driven | **Zero new** — it's the fly you already have | **Promoted.** Scientifically the most impressive option here, and it adds no new organism to be squeamish about |
| ***Ciona intestinalis*** larva | ~177 neurons, complete CNS (Ryan et al. 2016) — *needs licence/format verification* | Likely graded-ish | Swimming tadpole | **Lowest.** A translucent tadpole is closer to "cute" than anything else on this list | **Promoted to live option** under filter #2 |
| **Zebrafish larva** | Whole-brain *activity* imaging exists; whole-brain *synaptic* connectome does **not** | — | — | Low (it's a fish) | **Reject** — would be modelled, not measured. The one that would have solved filter #2 cleanly, and the data isn't there |
| **Mouse cortex (MICrONS)** | ~200k neurons, real EM — but a **fragment** of one animal's visual cortex, not a nervous system | — | No behaviour derivable | None | **Reject as a creature**; would make a spectacular brain-window-only easter egg |

### 6.1 The creep constraint is mostly an art-direction problem

Worth separating before the species choice gets made on vibes: **how disturbing
the pet is depends far more on how it's rendered than on which animal it is.**
The current fly is photoreal-ish — brown chitin, compound eyes, a textured
abdomen — which is a deliberate choice, not a requirement.

Three rendering registers, all of which run the same real connectome:

- **Literal** (today's fly). Maximum "whoa, that's a fly," maximum squeamishness.
- **Stylised** — accurate proportions and gait, but glassy/translucent materials,
  soft emissive palette, no texture detail. *C. elegans* is actually transparent
  in life, so this is the honest rendering, not a cop-out.
- **Diagrammatic** — the creature's body *is* its nervous system: 302 labelled
  points and their connections, arranged along the body, undulating along your
  window edges with activity visibly propagating down the motor chain.

The third one deserves serious consideration on its own merits, not just as
creep-avoidance. It fits the project's actual thesis better than a photoreal
animal does — the whole point is that the wiring is real — and it makes the
brain window and the pet the *same object* instead of two views bolted together.
A crawling data visualisation is also something nobody else has shipped.

**Recommendation under both filters:** *C. elegans*, rendered stylised or
diagrammatic, with the VNC/MANC "deepen the fly" work as the parallel track that
carries no creep risk at all. If a stylised worm still reads as unpleasant in
Phase 5's first render, *Ciona* is the fallback — same graded engine, same
crawler-ish body code, swap the data and the silhouette.

**Other readings of "give other option besides fly"** that I should not silently
rule out — tell me if I've picked the wrong one:

- **(A) A second species** — what this document assumes.
- **(B) Deepen the fly instead**: add the VNC connectome so the legs are real too.
- **(C) A cosmetic skin** (different insect, same fly brain) — cheap, but it makes the app dishonest, so I'd argue against it.
- **(D) "Other option" = a user-facing picker** — a tray menu "Creature ▸ Fruit fly / Worm". This is a *consequence* of (A), and the abstraction delivers it for free; if this is all you meant, §5 still stands and §6 gets simpler.
- **(E) An invented creature** — raised 2026-08-23. See §6.2.

### 6.2 Invented creatures

*Raised by Jesse 2026-08-23: "cant we fabricate a new animal or a fictional one
we make synthetic genome for."*

Short answer: **yes, and it solves more problems than it creates** — but there
are three different things this could mean, with wildly different costs, and one
factual correction.

**The correction, so it doesn't get designed around:** you cannot compute a
connectome from a genome. Nobody can — not for a fly, not for a worm, not for
anything. Genome → body plan → wiring is the central unsolved problem of
developmental biology. What *is* real and buildable is a **developmental growth
model**: a compact parameter seed that drives axon-guidance rules to grow a
wiring diagram. "Genome" is a fair metaphor for that seed as long as the app
never implies it's DNA.

**Why an invented creature is attractive here** — it retires three live risks
from §8 at once:

| Risk it kills | How |
|---|---|
| Dataset licence unverified (blocks Phase 5) | Nothing to license. It's yours. |
| Dataset access/format unknown | Nothing to download. |
| "Don't creep me out" (§6.1) | Every complete real connectome belongs to an invertebrate. An invented creature has no such constraint — you can design the silhouette to be radially symmetric, limbless, soft, and unlike anything that triggers a vermin reflex. |

It also makes an excellent **test fixture**: a 40-neuron synthetic creature
exercises the whole `Creature` trait in a unit test without loading 668 real
neurons.

#### The three versions

**(i) Fabricate a connectome outright.** Hand-author or randomly generate the
wiring. Cheapest, and the only one I'd argue against shipping standalone — see
the honesty rules below. Fine as a test fixture or a toy mode.

**(ii) Chimera — real circuits, recombined.** *Recommended.* Build an animal that
does not exist out of circuit modules that do: FlyWire's LC4/LPLC2 looming
detectors for vision, a *C. elegans*-style locomotor CPG for the body wave, real
motor pools, real sensory transduction — grafted together with synthetic
connective interneurons. **Every module is measured data; only the composition is
invented.** That is an honest and, as far as I know, novel thing to ship: "no
such animal exists; every circuit inside it does."

It also sidesteps the failure mode that kills version (iii): the modules are
pre-validated, so you know the looming detector detects looming before you wire
it to anything.

**(iii) Grown from a seed.** A compact "genome" — body-plan parameters (segments,
symmetry, limb placement), sensor complement and placement, neuron-type counts,
gradient fields, axon-guidance rules, E/I ratio, a connection-probability-vs-distance
kernel — fed to a developmental process that places somas, extends growth cones
down gradients, forms synapses where they meet, and prunes. The connectome is not
authored edge-by-edge; it *emerges*. Same seed → same creature, reproducibly.
Different seed → a different animal, forever.

This is the most fun version and the one with an open-ended scope. **The reason
it's a stretch goal and not Phase 5:** a grown network has no guarantee of doing
anything interesting. The fly works because evolution tuned it for 200 million
years *and* because `etl.py` deliberately selected populations with known
function. A randomly-grown network gives you silence, seizure, or noise —
`CLAUDE.md`'s "operating point is razor-thin" warning is exactly this problem in
miniature. Making it work means adding a **selection loop** (generate → simulate →
score behavioural richness → mutate), which is a genuinely interesting project
and is not a feature of a desktop pet. Budget it separately.

#### Honesty rules, if any of this ships

The project's entire value is that the brain is real. A synthetic creature
doesn't damage that *if the distinction is structural rather than a disclaimer
nobody reads*:

1. **Provenance is already in the design.** §5's `Connectome.provenance` was put
   there for licensing; it does double duty here — and it should move to
   **per-neuron and per-edge**, so a chimera can be honest at the granularity it
   actually mixes at:
   ```rust
   pub enum Provenance {
       Measured  { source: &'static str, version: &'static str, citation: &'static str },
       Grown     { generator: &'static str, seed: u64, matched: &'static [&'static str] },
       Authored,
   }
   ```
2. **The brain window does the explaining.** Real connectomes render at real soma
   coordinates — they look like a brain because they are one. A synthetic one has
   no anatomy, so render it as an obviously abstract force-directed graph. For a
   chimera, **colour by provenance**: measured edges warm, synthetic edges cool.
   You can see at a glance which parts of the animal are real. That is both the
   honesty mechanism and the best-looking thing in the app.
3. **Label it in the UI**: "Fruit fly — FlyWire v783" vs "«name» — synthetic,
   seed 0x…". Same in the README's measured-vs-modelled table.
4. **Never fabricate a real species.** An invented animal cannot be a lie about
   anything. A fake "honeybee connectome" would be a straightforward
   scientific-integrity failure, and the honeybee is exactly the kind of species
   someone will ask for because the real data doesn't exist. Hard no.

#### Cost

- **(ii) chimera:** ~1 week on top of the Phase 4 abstraction — it is a
  `Creature` impl plus a module-grafting step in the ETL. Nothing in §5 changes.
- **(iii) grown + selection loop:** 2–4 weeks and genuinely open-ended. Separate
  track, after a real second creature ships.

**Recommendation:** ship a real second creature first (it's what makes the
abstraction credible), then (ii) as the third creature — where the creep
constraint and the licence risk both go to zero and you get the
colour-by-provenance brain window as the payoff.

### 6.3 What would actually be most fun

*Asked 2026-09-05: "what would be the most fun and engaging?"*

Everything above optimises for correctness and cost. This section optimises for
whether anyone still has it running in a month — a different question, with a
different answer.

**What makes a desktop pet survive past week one.** Novelty is spent in about ten
minutes. What's left has to be one of: *it reacts to me in ways I discover*, *it
changes over time*, *I can see why it did that*, or *it's mine specifically*. The
existing app already nails the first and third — the cursor-lunge-versus-slow-
approach asymmetry is a genuinely great mechanic, and the brain window is the
only pet on earth that shows its reasoning. It has nothing for the second or
fourth. **That's the gap worth filling, and the two cheapest items on this whole
document fill it.**

Ranked by fun per unit of work:

**1. Habituation — the pet gets used to you. (~100 lines. Do this first.)**

The single highest-leverage feature in this document, and it is nearly free.
Add a slow-adapting depression term on the sensory→command pathway: repeated
harmless stimuli weaken their own drive, and it recovers over hours. Persist the
weights to disk between runs.

The result is a creature that flinches at your cursor for the first week and
gradually stops, *because you have never actually hurt it* — and that startles
properly again after you leave it alone for a weekend. You now have a history
with it. Nothing else on this list buys that much engagement for that little code.

It is also **real biology, not a gimmick**: habituation, dishabituation and
sensitization of the tap-withdrawal response are among the best-characterised
learning phenomena in *C. elegans*, and looming-response habituation is
documented in *Drosophila*. It belongs in the modelled-not-measured table —
the connectome gives wiring, not learning rules — but the phenomenon and its
circuit locus are real, so this is the same class of honest modelling choice as
the LIF dynamics already are.

**2. Glass anatomy — you can see the brain inside the body. (Material change + a point cloud you already render.)**

This is my answer to the open §6.1 rendering question. Not literal, not a bare
diagram: **a soft translucent body with the live connectome visible inside it,
firing as it moves.**

It wins on every axis at once:

- **Kills the creep problem permanently**, for every creature, forever. Glass and
  glow don't trigger a vermin reflex; chitin and compound eyes do. You stop
  having to pick species by squeamishness.
- **It's the app's thesis made visible.** Right now the pet and the brain window
  are two objects and you look back and forth between them. Fuse them and the
  reasoning is *in* the animal — you watch the escape command propagate and then
  the body goes.
- **It's the honest rendering for anything synthetic.** A grown creature has no
  real anatomy, so a photoreal body would be invented detail presented as fact.
  Glass isn't a stylistic compromise there; it's the only non-arbitrary choice.
- **It's the distinctive one.** Plenty of desktop pets exist. None of them are
  made of their own wiring.
- Cheap: the point-cloud pipeline is already being built for the brain window in
  Phase 3, and the body is already procedural. This is largely a material and
  a coordinate mapping.

Counter-argument worth taking seriously: a glowing wireframe is *cold*, and
people bond with faces. The answer is that the personality has to live in
**motion and reaction** rather than in eyes — closer to a firefly than a
Tamagotchi. The existing gait, grooming, startle and sleep behaviours are already
doing that work; they just need a silhouette you enjoy looking at.

**3. Seeded chimera — a creature that is specifically yours. (~1–2 weeks on top of Phase 4.)**

This is the fusion of §6.2 (ii) and (iii), and it **solves the failure mode that
made (iii) a stretch goal.** Don't grow a network from scratch — have the seed
select and compose *real, pre-validated circuit modules*: how many looming
detectors, how many CPG segments, which sensor complement, what body plan, what
E/I ratio, which gradients wire them together.

Every creature you roll is then **guaranteed to function, because its parts are
measured and already known to work.** The variation lives in composition and
proportion, not in whether the thing does anything at all. That is the whole
problem with grown-from-scratch networks — silence, seizure, or noise — and this
sidesteps it without needing a selection loop.

Then the engagement payload: **the seed is yours.** Hash a phrase you type, or
your machine name. Everyone gets a different animal, reproducibly — "seed
0xC0FFEE" is a creature you can send to someone and they get *the same animal*.
Two seeds can produce a child. Mutation is a slider.

**4. Everything else.** Breeding/lineage sits on top of 3. Full grown-from-scratch
plus a selection loop (§6.2 iii) becomes largely unnecessary once 3 exists — it
costs 2–4 weeks to reach a worse version of the same feeling.

#### The cheap experiment

Items 1 and 2 are **both testable on the existing macOS build, before any porting
happens.** Habituation is a term in `Sim.swift`; glass anatomy is materials in
`FlyModel.swift` plus the point cloud `BrainView.swift` already builds. If a Mac
is available to run it on, that is a day or two of work that tells you whether
the fun hypothesis is right *before* committing 5–8 weeks to a Rust port. If it
isn't, these move to just after Phase 3 — but they should not wait until Phase 5.

**Recommended running order, revised for fun:** Spike 0 → core+tests → Windows
shell → brain window → **habituation + glass anatomy** → abstraction → second
creature → seeded chimera.

---

## 7. Phased plan

Sequenced so the highest-risk unknown dies first and the science is verified
before a single pixel is drawn.

### Spike 0 — prove the overlay (1–2 days) · *gate on everything else*
Minimal Rust binary: transparent, click-through, always-on-top, no-taskbar
window; wgpu with `Dx12SwapchainKind::DxgiFromVisual`; a spinning lit cube with a
soft shadow over the desktop; drag a window under it; move it to a second monitor
at a different DPI; check GPU/CPU cost at 120 fps.
**Exit:** it looks right and costs <5% CPU. **If it fails → switch the shell to
Godot 4 and continue; `core/` is unaffected.**

### Phase 1 — the core, headless (3–5 days)
Port `Sim.swift`, `SignalBuilder`, `brainBehavior` and the gait/flight math into
`core/`. Port **both test suites first**.
**Exit:** `cargo run -- --simtest` and `--behaviortest` pass with the same
invariants `CLAUDE.md` names — GF silent over 4 s of rest, GF fires ≤ ~10 ms
after an abrupt loom, walk-drive duty 20–50%, siesta (scale 0.84) walk-drive >3%,
all 17 behaviour checks. Numbers compared side-by-side against the Swift build.

### Phase 2 — Windows shell, fly only (8–12 days)
Renderer (meshes, hierarchy, Blinn-Phong, one shadow map, orthographic camera);
`platform/windows.rs` (overlay, tray, `EnumWindows` + DWM terrain,
`GetLastInputInfo`, click polling, power-info thermal proxy, monitor
enumeration); `ScreenSpace` conversion type **with unit tests** — this is the
highest-bug-density item in §4.
**Exit:** a fly walks on the Windows desktop, lands on real window ledges, flees
a cursor lunge, rides a dragged window, sleeps at night. Tray menu at parity with
the mac one. `--snapshot` works.

### Phase 3 — brain window (3–4 days)
Point-cloud pipeline (23k points, screen-space sizing, additive blend), spike
flashes, ray-unproject click-to-stimulate, region labels, `--brainshot`.
**Exit:** clicking the Giant Fiber makes the fly escape.

### Phase 4 — creature abstraction (3–5 days)
Refactor to §5. Roles become a data manifest. `Sim` becomes a trait. The fly
becomes `creatures/drosophila`. **No behaviour change is permitted in this phase.**
**Exit:** both suites still pass, unchanged, on the refactored code. This is a
pure-refactor gate — if the numbers move, the refactor is wrong.

### Phase 5 — second creature (6–10 days, depending on §6)
Worm path: `GradedSim` integrator + its own test suite; ETL for the chosen
dataset (with licence check); role manifest; crawler body with proprioceptive
undulation; mechanosensory/thermotactic/tap-habituation transduction; brain view
with per-neuron labels; tray "Creature ▸" picker.
*The VNC "deepen the fly" track is ~50% cheaper — it reuses `LifSim`, the
existing body and the existing senses, and needs only ETL + a motor-neuron→leg
mapping. If Phase 5 needs to be short, that is the version to run.*
**Exit:** a new `--simtest`/`--behaviortest` pair for the second creature, with
its own invariants (e.g. "anterior touch → reversal within N ms", "tap response
habituates over 10 taps", "undulation frequency tracks `VB/DB` output").

### Phase 6 — macOS parity + packaging (4–6 days)
`platform/macos.rs` via `objc2`; retire the Swift app (or keep it tagged as the
reference implementation — recommended, it is the oracle for the port). Icons,
signing, installer, per-platform README honesty edits, licence files for the
second dataset.

**Total: 5–8 weeks solo and focused.** The long poles are the renderer (Phase 2)
and the second dynamics engine (Phase 5) — both shrink materially under the
Godot shell / larva-instead-of-worm choices respectively.

---

## 8. Open decisions & risks

### Decisions

1. ~~**Shell: Rust, or Godot 4?**~~ — **DECIDED 2026-08-23: Rust, gated on
   Spike 0.** Godot 4 remains the documented fallback and §2 keeps the full
   comparison, because if Spike 0 fails this decision gets revisited on the spot.
2. **Second creature** — *Drosophila* larva **eliminated 2026-08-23** on the
   "not creepy" constraint. Live options: *C. elegans* (stylised or
   diagrammatic), *Ciona* larva, or the VNC "deepen the fly" track. See §6 and
   §6.1; the answer changes Phase 5 by roughly a factor of two.
   **§6.1's rendering-register question gates this and should be answered first.**
3. **Does macOS stay alive on the new core?** — recommended yes; it is also your
   only oracle for verifying the port. If no, option (c) in §2 (C# + Silk.NET)
   becomes competitive again.
4. **DPI policy**: does the fly keep a constant *apparent* size across monitors
   (recommended) or a constant pixel size?
5. **Naming**: "DesktopFly" stops being true the moment there are two creatures.

### Risks

| Risk | Severity | Mitigation |
|---|---|---|
| Transparent GPU overlay behaves differently across GPU vendors / Windows builds | **High** | Spike 0 first; test on Intel/AMD/NVIDIA; Godot fallback |
| Coordinate-system and DPI bugs (y-flip, negative virtual-screen coords, per-monitor scale) | **High** | One `ScreenSpace` type, unit-tested; never inline a y-flip |
| Transliteration silently changes sim behaviour (razor-thin operating point per `CLAUDE.md`) | **High** | Port tests first; compare numbers against the Swift build phase by phase; treat constants as data |
| Global mouse hook trips antivirus/EDR heuristics | Medium | Poll `GetAsyncKeyState` instead of `WH_MOUSE_LL` |
| Second dataset's licence is incompatible with redistribution | Medium | Check **before** Phase 5; `Provenance` is a required field; keep the code/data licence split |
| Renderer scope creep (SceneKit does more than it looks) | Medium | `three-d` in the back pocket; brain window deferred to Phase 3 |
| Overlay fights fullscreen games/presentations | Low | `SHQueryUserNotificationState` → hide |
| Fork diverges permanently from upstream `DenisSergeevitch/desktop-fly` | Low | It already will — a rewrite plus a creature abstraction is not mergeable upstream. Decide now whether to keep the Swift app tagged as the reference. |

### Things I did **not** verify and you should not treat as settled

- Redistribution terms for the *C. elegans* dataset (the FlyWire CC BY-NC split
  is known; this one is not). **Check before Phase 5.**
- The *Ciona* connectome — I have it as ~177 neurons, complete CNS, Ryan, Lu &
  Meinertzhagen 2016 (*eLife*), from memory and not from a source I checked in
  this pass. **Verify the neuron count, completeness and licence before
  promoting it past "fallback."**
- Whether the MANC/VNC connectome is redistributable on the same terms as
  FlyWire, and its data-access format — this now matters more, since the
  "deepen the fly" track is a live Phase 5 option.
- Access format and download path for the Winding et al. larval connectome —
  the paper is `Science` 379, eadd9330. Moot unless the larva comes back.
- On-hardware behaviour of the wgpu DirectComposition path — documented, not
  yet tested on your machine. That is what Spike 0 is for.

---

## Appendix — verified sources

- wgpu DirectComposition / transparent windows on DX12:
  [`Dx12SwapchainKind` docs](https://docs.rs/wgpu-types/latest/wgpu_types/enum.Dx12SwapchainKind.html) ·
  [`Dx12BackendOptions`](https://wgpu.rs/doc/wgpu/struct.Dx12BackendOptions.html) ·
  [prior breakage, gfx-rs/wgpu#7108](https://github.com/gfx-rs/wgpu/issues/7108) ·
  [portable transparency discussion, gfx-rs/wgpu#3486](https://github.com/gfx-rs/wgpu/issues/3486)
- Godot tray + windowing:
  [`DisplayServer` docs](https://trinovantes.github.io/godot-docs/classes/class_displayserver) ·
  [status indicator PR, godotengine/godot#80211](https://github.com/godotengine/godot/pull/80211)
- Windows composition swapchain:
  [`IDXGIFactory2::CreateSwapChainForComposition`](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgifactory2-createswapchainforcomposition)
- *C. elegans* connectome, graded signalling, gap junctions:
  [Raising the Connectome (Front. Cell. Neurosci. 2020)](https://www.frontiersin.org/journals/cellular-neuroscience/articles/10.3389/fncel.2020.524791/full) ·
  [multilayer connectome (arXiv 1608.08793)](https://arxiv.org/pdf/1608.08793) ·
  [synaptic signalling connectome for locomotion (PLOS Comp Biol)](https://journals.plos.org/ploscompbiol/article?id=10.1371%2Fjournal.pcbi.1005834)
- *Drosophila* larval brain connectome:
  [Winding et al., *Science* 379, eadd9330 (2023)](https://www.science.org/doi/10.1126/science.add9330) ·
  [MRC LMB summary](https://mrclmb.ac.uk/news-events/articles/complete-synaptic-resolution-connectome-of-an-insect-larval-brain/)
- Existing data provenance (unchanged): FlyWire FAFB v783 — Dorkenwald et al.,
  *Nature* 634, 124–138 (2024); Schlegel et al., *Nature* 634, 139–152 (2024).
