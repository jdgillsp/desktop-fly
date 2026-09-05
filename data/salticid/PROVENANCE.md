# data/salticid — provenance

This is the circuit of a creature that does not exist (see `SPIDER_PLAN.md`).

| part | count | origin | licence |
|---|---|---|---|
| neurons, `origin: measured` | 789 | FlyWire FAFB v783, extracted by `etl_chimera.py` exactly as `etl.py` extracts the fly's | CC BY-NC 4.0 (`../DATA_LICENSE.md`) |
| edges, fourth column `0` | 26110 | FlyWire FAFB v783 synapse counts, signed by neurotransmitter | CC BY-NC 4.0 |
| neuron `authored:pounce` | 1 | **authored** — no such neuron in any animal | MIT (code licence) |
| edges, fourth column `1` | 127 | **authored** — LC11 → pounce, weight 8.0 each | MIT |

`brain_points.json` is not duplicated here; the loader falls back to the fly's,
because the measured modules *are* the fly's and their somas sit where they sit.

The measured modules: LC4/LPLC2 (looming), DNp01 (giant fiber), DNa01/DNa02
(steering), DNp09 (walking), DNg11 (grooming), MDN (backing up), and LC11
(small-object detection) with its strongest downstream partners. The fly's
wing module (DNp02/04/11) is omitted.
