<p align="center">
  <img src="assets/fly.png" width="340" alt="DesktopFly — a 3D fruit fly">
</p>

<h1 align="center">DesktopFly 🪰</h1>

<p align="center">
A 3D fruit fly that lives on your macOS desktop — driven by a live spiking
simulation of the real <a href="https://codex.flywire.ai">FlyWire</a>
connectome. It walks across your windows, grooms, sleeps, and decides to flee
your cursor with the same neurons a real fly uses.
</p>

<p align="center">
  <img src="assets/brain.png" width="560" alt="Live brain window: 23,210 real neuron positions, spikes flashing">
</p>

<p align="center"><sub>
The fly's brain window: 23,210 real neuron soma positions from FlyWire v783,
with live spikes flashing at real neuron locations. The two glowing yellow
markers are the Giant Fibers — the escape command neurons. Click any region
to stimulate it.
</sub></p>

## What's real

- **23,210 neuron soma positions** (of 139,255 in FlyWire v783) render the
  rotating brain window, colored by super-class (FlyWire's coarse cell-type
  grouping).
- **A 668-neuron circuit with ~19,000 real synaptic connections** (synapse
  counts, signed by neurotransmitter prediction) runs as a 1 kHz
  leaky-integrate-and-fire (LIF) simulation:
  - **LC4 (104) + LPLC2 (210)** looming-detector visual neurons
  - **DNp01 / Giant Fiber (GF) (2)** — the escape command neuron
  - **DNa01 + DNa02 (4)** steering neurons · **DNp09 (2)** forward walking
  - **DNg11 (6)** grooming · **MDN (4)** backward walking ("moonwalker")
  - **DNp02/DNp04/DNp11 (6)** escape-maneuver (wing) neurons
  - their 330 strongest partners, including ascending (proprioceptive) and
    sensory (wind) neurons
- **Escape is not scripted.** Your cursor's approach becomes looming input to
  the real LC4/LPLC2 cells; the fly takes off only when the Giant Fiber
  actually spikes through its real synapses — ~1,200 synapses of feedforward
  inhibition push back, which is why slow approaches are tolerated and fast
  lunges trigger escape in ~4 ms, just like the real animal.

The body itself is procedural (FlyWire is a brain connectome — no body
geometry exists), with a tripod gait, visible wing-beat, altitude-scaled
flight, grooming, and sleep postures.

## Installation

Requirements: **macOS 13+**, Xcode Command Line Tools (Swift 5.9+).
No permissions or entitlements needed — everything it senses
(cursor, window frames, clicks-as-taps, thermal state) is permission-free.

```sh
git clone https://github.com/DenisSergeevitch/desktop-fly.git
cd desktop-fly
./build.sh
./DesktopFly
```

A 🪰 item appears in the menu bar; quit from there. The fly wanders your
desktop on a transparent, click-through overlay — it never intercepts your
mouse or keyboard.

## Controls (menu bar 🪰)

| item | effect |
|---|---|
| Pause / Resume | freeze the world |
| Show/Hide Brain | toggle the live brain window |
| Escape Test (loom) | inject a looming stimulus, watch the GF fire |
| Move to Next Display | hop the fly across monitors (shown when >1 display) |
| Add / Remove Fly | extra flies (only fly #1 carries the brain) |
| Scare Flies | startle everyone |

**The brain window is interactive**: hovering pauses the rotation; clicking a
region "optogenetically" stimulates the ~60 nearest circuit neurons for
400 ms. The fly's reaction is whatever the real network does downstream —
click the Giant Fiber and it escapes; click DNg11 and it grooms; click one
side's DNa01/02 and it turns.

## How real neurons drive the body

| body behavior | driven by |
|---|---|
| escape takeoff | DNp01 giant fiber spike |
| walk vs. rest, walking speed | DNp09 rate |
| steering | DNa01+DNa02 left−right rate difference |
| grooming | DNg11 rate |
| backward scoot | MDN burst |
| nervous darting | LC4/LPLC2 population rate |
| wing-beat effort, threat wing-raise | DNp02/04/11 rate |
| spontaneous takeoff | whole-population arousal |

The loop also closes body→brain: the gait rhythm feeds the circuit's real
ascending (proprioceptive) neurons in phase with the legs, and fast cursor
motion stimulates its sensory (wind) partners.

## Desktop ecology (all permission-free macOS senses)

- **Window terrain**: window top edges are ledges — the fly lands on them,
  walks along them, rides a window you drag, and startles when one closes
  under its feet.
- **Window looms**: a window appearing near the fly feeds the looming
  pathway; the circuit decides whether to flee your dialogs.
- **Clicks are substrate taps**; clicking next to the fly startles it through
  the wind→GF pathway. **Typing is vibration** (idle-time API — knows *when*
  keys were pressed, never which).
- **Circadian rhythm**: dawn/dusk activity peaks, midday siesta, night
  quiescence. **Sleep**: idle at night → it sleeps, breathing slowly, with
  raised arousal threshold; it grooms after waking.
- **Temperature**: flies are ectotherms — a hot Mac is a faster fly.

## Regenerating the data

`data/` ships with compact derived files. To rebuild them from the raw
FlyWire Codex dumps (~60 MB download):

```sh
mkdir -p /tmp/flywire && cd /tmp/flywire
B=https://storage.googleapis.com/flywire-data/codex/data/fafb/783
curl -O "$B/classification.csv.gz" -O "$B/coordinates.csv.gz" \
     -O "$B/connections.csv.gz" -O "$B/consolidated_cell_types.csv.gz"
cd - && python3 etl.py /tmp/flywire
```

## Diagnostics

```sh
./DesktopFly --simtest        # circuit invariants: GF silent at rest, 4 ms loom latency, ...
./DesktopFly --behaviortest   # 17 end-to-end checks: stimulate neurons -> body reacts
./DesktopFly --snapshot f.png  # offscreen fly render
./DesktopFly --brainshot b.png # offscreen brain render
```

## Creature #3: a jumping spider that does not exist (Windows build)

<p align="center">
  <img src="assets/windows-spider.png" width="240" alt="The glass jumping spider, circuit crackling inside">
  <img src="assets/windows-spider-literal.png" width="240" alt="The same spider in the literal register">
</p>

The Rust build (`rust/`) can run a third creature, picked from the tray or
with `--creature salticid`: a **jumping spider** built for people who code.
It watches you from a window ledge, turns its head toward your cursor and
stalks it, abseils off ledges on a dragline, jumps away from a lunge, and
pounces on "bugs" — three drifting points that appear when you tell it a
build failed (`desktopfly notify fail`; `notify pass` sends them off).

**No spider connectome exists**, so this animal is a labelled **chimera**:
every circuit inside it is measured FlyWire data, recombined; the animal is
invented. Its 790-neuron circuit is the fly's looming, escape, steering,
walking, grooming and backing-up modules (the wing module dropped) plus
**LC11 (127)**, FlyWire's small-object motion detectors, with their strongest
downstream partners — and exactly **one authored neuron**, the pounce node,
with **127 authored edges** from LC11. The brain window colours it cool and
draws it larger so it can never hide among real neurons; the tray and the
brain window call it a chimera, never a spider brain. See
[`SPIDER_PLAN.md`](SPIDER_PLAN.md) and
[`data/salticid/PROVENANCE.md`](data/salticid/PROVENANCE.md).

| spider behavior | driven by | status |
|---|---|---|
| escape jump (dragline first) | DNp01 giant fiber spike | measured wiring |
| stalking, walking speed, steering, grooming, backing away | DNp09 / DNa01+02 / DNg11 / MDN, as the fly | measured wiring |
| prey detection | LC11 population rate | measured wiring; size tuning modelled in the transduction |
| head orientation toward prey | LC11 left−right rate | **modelled readout** (LC11 does not reach the steering DNs in the extract) |
| the pounce | the authored pounce node | **authored** |
| settling while you work, bugs on a failed build | foreground app *class*, `notify` hook | senses; content-blind |

Its own suites: `dfcore --creature salticid --simtest` (14 circuit checks:
the giant fiber still fires 4 ms after an abrupt loom; LC11 ignores looms;
the pounce node is silent at rest and under looms, fires only under
small-object drive, and never fires the giant fiber) and `--behaviortest`
(14 end-to-end checks, including bug → LC11 → pounce → capture).

## What's modeled vs. measured

Honesty section: the connectome gives wiring, not physiology. The LIF
dynamics, neurotransmitter signs (ACh+, GABA−, Glu−), the gap-junction boost
on LC→GF and wind→GF (documented electrical coupling), synaptic delays, and
the sensory transduction (cursor → looming value) are standard modeling
choices layered on the real graph. Everything downstream of the sensory
neurons — who connects to whom, and how strongly — is FlyWire data.

The Rust build adds, all labelled as such in code and UI: **habituation**
(a modelled learning rule; the phenomenon is real, the connectome does not
carry it); and for the chimera, the **small-object size tuning** presented to
LC11 (the lobula is outside the extract), the **head-orientation readout**
of LC11's left−right rate, and the **authored pounce node** — 1 neuron and
127 edges out of 790 neurons and 26,237 edges, counted by the data, not by
a constant.

## License & citation

Code is MIT. The files in `data/` are derived from FlyWire (FAFB v783) and
are **CC BY-NC 4.0** — see [data/DATA_LICENSE.md](data/DATA_LICENSE.md).
If you use this, cite:

- Dorkenwald, S. et al. *Neuronal wiring diagram of an adult brain.* Nature 634, 124–138 (2024). https://doi.org/10.1038/s41586-024-07558-y
- Schlegel, P. et al. *Whole-brain annotation and multi-connectome cell typing of Drosophila.* Nature 634, 139–152 (2024). https://doi.org/10.1038/s41586-024-07686-5
