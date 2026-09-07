# data/weaver — provenance

The one circuit shared by the three web-building chimeras — creatures that do
not exist (see `WEB_PLAN.md` §3). Three species, one wiring, because there is
no spider wiring to differ in.

| part | count | origin | licence |
|---|---|---|---|
| neurons, `origin: measured` | 662 | rows of the fly's shipped FlyWire FAFB v783 extract (`../circuit.json`), minus the wing module, copied by `etl_weaver.py` | CC BY-NC 4.0 (`../DATA_LICENSE.md`) |
| edges, fourth column `0` | 17922 | FlyWire FAFB v783 synapse counts, signed by neurotransmitter, as in the fly's extract | CC BY-NC 4.0 |
| neuron `authored:strike` | 1 | **authored** — no such neuron in any animal; a vibration sense with no synapses, driven by the modelled transduction | MIT (code licence) |
| edges, fourth column `1` | 0 | — | — |

`brain_points.json` is not duplicated here; the loader falls back to the fly's,
because the measured modules *are* the fly's and their somas sit where they sit.

The measured modules: LC4/LPLC2 (looming, which is how a shadow over the web
reaches the giant fiber), DNp01 (giant fiber: drop on the dragline), DNa01/DNa02
(steering), DNp09 (walking), DNg11 (grooming), MDN (backing up), and the fly's
16 mechanosensory partners (a knock on the web, onto the giant fiber, as the
data wires them). **There is no LC11** — these animals do not hunt by sight.
The wing module (DNp02/04/11) is omitted. Prey vibration reaches only the
authored strike node, through the modelled transduction, never the measured
wiring.

The **web construction program has no neurons in it at all**; it is a
hand-written motor program, labelled PROCEDURAL in the tray and brain window.
