# Silk placement, construction movement, and physics validation

**Correction status (2026-09-07): the 27-scenario movement/deposition regression passes.**

The corrected run is `output/web-construction-fixed.json`. All 27 builds finish,
the largest physical construction step is 5.00003 units at 30 Hz (the configured
150 units/second limit), and the largest measured deposition gap is 0.245 units.
No fixed node lacks a support, and newly fixed pins move zero units when the
support mapper first processes them. The ordinary
`construction_movement_and_deposition_regression` test asserts completion,
movement bounds, physical deposition distance, support identity, pin stability,
gravity release, frame-rate consistency, and tension response.

Construction now uses persistent routes over connected graph edges, surface
travel, and explicit safety-line descents. Physical arrival gates attachment;
newly deposited nodes start at the spider's physical contact. Visual crossings
do not connect unrelated threads. Gait uses physical movement. Funnel planning
retries feasible bounds at the actual support or seeks a new site.

Strands retain material length across support motion and splitting. The compliant
tension-only solver transfers spider/prey loads, reports force/strain-derived
tension, and breaks sustained excessive strain. Separate tests cover load-induced
sag, retained material, breakage, disconnected crossings, and furniture contacts.
Furniture contact uses bark slabs, twig capsules, and pebble envelopes; obstructed
strands gain actual graph contact nodes while retaining material length. Rendered
silk uses that same graph instead of adding an unrelated sag curve.

The audit now measures new-knot coordinates at deposition, before physics can
move or prune them. This also fixes an audit-only stale-index panic introduced
when physical breakage was enabled. Broken loose fragments are allowed to remain
and fall; two house-spider runs retain 4 and 6 disconnected fragment edges. They
are not counted as fixed supports. The old chart-only unattached-travel metric is
retained as diagnostic data, not a verdict on the new physical routes.

Limits: these are game-scale physics and approximate solid-furniture envelopes,
not calibrated biological silk parameters or full mesh/self-collision. Contacts
are sampled and subdivision is bounded for frame cost. Live arbitrary desktop
layouts and every possible furniture arrangement are outside this deterministic
fixture matrix.

Final verification: 184 core unit tests, 12 integration tests, and 165 shell
tests passed (one diagnostic intentionally ignored). The strengthened pin
relocation regression also passed separately. The release executable was built
at `rust/target-web/release/desktopfly.exe`; orb, house-spider, and funnel
snapshot smoke checks all rendered successfully.

## Original failing baseline (retained for comparison)

**Original verdict: FAIL — before the corrections above.**

The earlier tests established that webs could finish and that coordinates stayed
finite. They did not establish that the animal reached each knot or followed a
continuous, supported path. This audit checks those properties separately.

## Reproduction

From `F:\projects\desktop-fly\rust`, in PowerShell:

```powershell
$env:DESKTOPFLY_WEB_AUDIT='F:\projects\desktop-fly\output\web-construction-audit.json'
cargo test --release -p dfshell audit_construction_supports_and_movement -- --ignored --nocapture
```

Use `C:\Users\twayf\.cargo\bin\cargo.exe` if Cargo is not on PATH.
The ignored diagnostic deliberately catches scenario panics to finish the report.
Its test-runner `ok` means the audit ran, **not** that the system passed validation.
Results are in `output/web-construction-audit.json`; the full execution log is in
`output/web-construction-audit.log`.

## Scope and measured results

27 deterministic builds: three species, seeds 1/5/12, bare habitat, furnished
habitat, and a desktop fixture with three UI control rectangles inside a window.
Each runs at 30 Hz for at most 900 simulated seconds, including the spatial solver.
Measurements use scene units, not physical millimeters or display pixels.

| Check | Result |
| --- | --- |
| Build completion | 25/27; two desktop funnel runs panic |
| Physical steps >25 units while logical advance is <=5.1 units | 2,939 samples across the completed runs |
| Largest physical step during building | 228.92 units in 1/30 second |
| Newly created local knots >12 physical units from spider | 27; largest gap 162.30 units |
| Final fixed nodes without a recorded support | 0 in completed runs |
| Final threads disconnected from all fixed nodes | 0 in completed runs |
| Moving without nearby chart silk, outline, or trailing line | 4,675 screening samples |

The last metric is a routing warning, not a general 3D collision verdict. The audit
does not model every glass face or furniture surface. The knot threshold is an
explicit measurement threshold, not a species-calibrated leg reach. The largest
gaps and discontinuities are much larger than the depicted animal.

## Blocking findings

1. **Physical strand switching teleports the spider.** `Silk::spatial_at`
   (`rust/core/src/silk.rs:185`) independently chooses the nearest strand in the
   2D chart each frame. `webspace::step` (`rust/shell/src/webspace.rs:175`) directly
   replaces the physical body position with that projection. Nearby/crossing
   chart strands can be far apart in 3D. There is no persistent strand contact or
   physical travel constraint. Example: furnished Parasteatoda, seed 1, at
   5.3667 seconds in the tangle stage: logical advance 5 units, physical advance
   48.18 units. Its maximum physical step is 228.92 units.

2. **Attachment is authorized by chart arrival, not physical arrival.**
   `rust/core/src/weaver.rs:1211` applies knot operations as soon as `walk_toward`
   reaches the 2D target. The body is positioned afterward through an independent
   projection. New knots can therefore appear far from the physical spider.
   The audit samples new, nonsplit knots within 2 chart units of the spider and
   measures their physical distance after the actual runtime projection.

3. **Construction travel has no support-routing requirement.**
   `rust/core/src/weaver.rs:618` advances directly toward a target. It does not
   route along the silk graph or actual habitat surfaces. A return to a retreat
   can cross empty space. Smoothing the physical projection alone would hide
   teleports without correcting this problem.

4. **UI-anchored funnel bounds can invert and crash.** Desktop-controls
   Agelenopsis seeds 1 and 12 panic. Seed 1 reports clamp limits `342 > 169.2`.
   `FunnelProgram::reset` chooses a real support that can be outside its planning
   rectangle, then clips sheet bounds independently
   (`rust/core/src/funnel.rs:343`, `:353`, `:354`). `queue_filling` later clamps
   into the inverted interval (`:211`). An infeasible site must be replanned;
   silently swapping limits would not establish a physically valid sheet.

## Physics checks

| Probe | Result |
| --- | --- |
| Two pinned ends remain fixed under gravity | PASS |
| Free center junction sags | PASS: center falls from z=40 to z=37.30494 |
| Same four-second bridge simulation at 30/60/120 Hz | PASS: identical center height in this fixture |
| Releasing all pins lets the silk reach the floor | PASS |
| Stretching supports 50% changes reported strand tension | FAIL: both strands remain at tension 1.0 |

The solver integrates gravity and damped velocities and applies unilateral
length constraints. It is a useful foundation, but is not the complete physics
the desired behavior requires:

- `Thread::tension` is not derived from physical stretch or solver forces.
- The solver derives lengths from node rest positions; pinned node rest positions
  are reset from the support mapping each step (`rust/core/src/silk.rs:143`,
  `:164`). A material rest length must belong to the laid strand and survive
  support motion and splitting.
- Spider and prey mass do not load the spatial solver; they are placed onto its
  output afterward. The animal cannot sag or tension the web through its weight.
- Contact constraints do not resolve furniture intersections. The existing floor
  and x/y boundary clamps are not general habitat collision handling.
- The renderer adds a sag curve (`rust/shell/src/weaverbody.rs:515`) that is not
  part of the collision/contact graph. Rendering and contact should use the same
  physical strand shape.

## Acceptance criteria for correction

Persist the animal's physical support/contact and route over connected strands
or actual surfaces. Gate knot creation on physical arrival. Model explicit
bridge casting, abseiling, and silk payout for transitions that cannot be walked.
Give each strand persistent material length, compliance, strain/force-derived
tension, load transfer from the animal/prey, and damage behavior. Resolve contact
against habitat surfaces and use the same geometry for rendering and interaction.
Reject infeasible UI sites before scheduling construction.

Then rerun this audit and promote the corrected cases to ordinary regression
tests. Completion alone must not count as a pass for movement or physics.
