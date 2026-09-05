# Spike 0 — results

**Verdict: PASS.** The Rust shell is viable; PORT_PLAN.md §8 decision 1 stands
(Rust, not the Godot fallback).

Run on: Windows 11 Pro 26200, AMD Radeon integrated (Dx12), 2560×1440 @60 Hz
primary, dual-monitor (virtual desktop extends to x=5120).
Toolchain: rustc 1.98.0, wgpu 30.0.1, winit 0.30.13, windows 0.62.2.

## Criteria

| # | Criterion | Result | Evidence |
|---|---|---|---|
| 1 | Transparent (per-pixel alpha) | **PASS** | `alpha_modes: [Auto, Inherit, Opaque, PostMultiplied, PreMultiplied]`, chose `PreMultiplied`. Screenshot shows desktop text legible *through* the blob at α=0.92, edges clean (no premultiply halo). |
| 2 | Click-through | **PASS** | `WS_EX_TRANSPARENT` read back set via `winprobe`. Verified as a style bit, not by a synthetic click. |
| 3 | Always-on-top | **PASS** | `WS_EX_TOPMOST` set; renders over document windows. |
| 4 | No taskbar / Alt+Tab entry | **PASS** *(after a fix)* | `WS_EX_TOOLWINDOW` set — see Finding 3. |
| 5 | GPU-composited | **PASS** | DX12 backend, `Dx12SwapchainKind::DxgiFromVisual` (DirectComposition), `WS_EX_NOREDIRECTIONBITMAP` set. |
| 6 | Cheap | **QUALIFIED** | 7.8% of one core, 297 MB working set, locked 60 fps at full-screen 2560×1440. Above the <5% target — see Finding 6. |

## The controlled experiment

The mechanism was falsified, not assumed. Running `--hwnd`
(`Dx12SwapchainKind::DxgiFromHwnd`, no `WS_EX_NOREDIRECTIONBITMAP`) on the same
machine, same frame, same everything:

- alpha modes collapse to `[Opaque]`
- the screen goes **entirely opaque black**

So the transparency is specifically attributable to the DirectComposition
presentation path — not to winit's window flags, not to a driver quirk.

## Findings that change Phase 2

1. **Pin the backend to DX12 on Windows.** With `Backends::DX12 | VULKAN`, wgpu
   picked Vulkan, which reported `alpha_modes: [Opaque]` and silently defeated
   the overlay. The transparent path is a DX12-backend feature here.

2. **`Limits::downlevel_defaults()` caps `max_texture_dimension_2d` at 2048**,
   which is smaller than an ordinary monitor — surface configuration panicked at
   2560×1440. A full-screen overlay must request `adapter.limits()`.

3. **winit's `with_skip_taskbar(true)` is not `WS_EX_TOOLWINDOW`.** It uses
   `ITaskbarList::DeleteTab`, which removes the taskbar button but leaves the
   window in Alt+Tab. The platform layer must set `WS_EX_TOOLWINDOW` explicitly
   (plus `WS_EX_NOACTIVATE` so the overlay never takes focus). Together these are
   the real equivalent of macOS `setActivationPolicy(.accessory)` (main.swift:891).

4. **winit owns `GWL_EXSTYLE` and will clobber external changes.** It keeps a
   cached flag set and rewrites the whole value on calls such as
   `set_cursor_hittest`. Styles set *before* those calls read back as unset.
   Fix: apply after every winit window call, and re-assert once the window is
   live. In the real app the re-assert belongs in the ~1 Hz window-poll timer.

5. **`DWMWA_EXTENDED_FRAME_BOUNDS` is measurably different from `GetWindowRect`**,
   as PORT_PLAN.md §4 predicted. A maximized window reports `GetWindowRect` top
   at **y = −8**; the fly would walk 8 px above the visible title bar. `winprobe
   --terrain` found 21 real ledges across both monitors using the DWM bounds.

6. **A full-screen overlay is the wrong shape on Windows.** 7.8% of a core to
   clear and present 3.7 M pixels for one small creature. The macOS build gets
   away with it; here it is worth sizing the window to the creature's bounding
   box plus a margin and moving it, with the full virtual-desktop rect used only
   as the coordinate space. Ledges and window terrain are unaffected. **Decide
   before Phase 2's renderer work.**

## Reproduce

```
cargo build --release -p spike0 -p winprobe
target/release/spike0.exe --seconds 15          # the overlay
target/release/spike0.exe --seconds 15 --hwnd   # the control: opaque black
target/release/winprobe.exe --terrain           # ledges + overlay style audit
```
