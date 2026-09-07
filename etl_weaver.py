#!/usr/bin/env python3
"""Build the shared circuit of the three web-building chimeras (WEB_PLAN.md §3.1).

The weavers -- an orb weaver, a gumfoot-tangle weaver, a sheet-and-funnel
weaver -- differ in body and construction program, not in wiring, because no
spider connectome exists to differ in. So they share one labelled chimera,
written to data/weaver/ and never touching the fly's shipped files.

Unlike etl_chimera.py, this reads the fly's *shipped* extract, data/circuit.json,
rather than the raw Codex dumps (which are not kept on this machine). That is
deliberate and reproducible: every measured element here is exactly a row of
the fly's extract, so the provenance chain is one step long.

  1. Core populations: the fly's minus the wing module (DNp02/04/11: no wings).
     NO LC11: the weavers hunt by web vibration, not by sight, so the salticid's
     visual prey pathway is not lifted across just because it exists.
  2. Partners: the fly's, unchanged -- including its 16 mechanosensory
     partners, which are the measured wind/tap input onto the giant fiber and
     are the only way a vibration reaches this circuit.
  3. One AUTHORED neuron -- the strike node -- with NO edges. It stands in
     for a spider's slit sensilla, which have no counterpart in the fly
     extract: the modelled vibration transduction drives it directly, on a
     slow membrane (the manifest gives it a 1 s time constant and its own
     threshold), so a sustained struggle accumulates on it. A knock does not
     go through it at all: it goes through the fly's measured mechanosensory
     partners onto the giant fiber, as the data wires it. The first draft
     gave the node authored edges from those partners; across seeds that
     inherited their resting rate and their coupling to GF, and no weight
     separated a struggle from a knock (WEB_PLAN.md §9.1). Neurons carry
     "origin": "measured" | "authored"; edges are [pre, post, weight, origin]
     with origin 1 = authored -- there are none in this file.

Usage: python3 etl_weaver.py
"""
import json, os
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "data", "circuit.json")
OUT = os.path.join(HERE, "data", "weaver")
os.makedirs(OUT, exist_ok=True)

DROP_ROLES = {"escw"}

with open(SRC, encoding="utf-8") as f:
    fly = json.load(f)
neurons_in, edges_in = fly["neurons"], fly["edges"]
print(f"fly extract: {len(neurons_in)} neurons, {len(edges_in)} edges")

keep = [i for i, n in enumerate(neurons_in) if n["role"] not in DROP_ROLES]
remap = {old: new for new, old in enumerate(keep)}
neurons = []
for old in keep:
    n = dict(neurons_in[old])
    n["origin"] = "measured"
    neurons.append(n)
edges = []
for e in edges_in:
    a, b = remap.get(int(e[0])), remap.get(int(e[1]))
    if a is None or b is None:
        continue
    edges.append([a, b, e[2], 0])
print(f"measured: {len(neurons)} neurons, {len(edges)} edges (dropped {len(neurons_in) - len(neurons)} wing neurons)")

sens_idx = [i for i, n in enumerate(neurons) if n["role"] == "other" and n["type"] == "sensory"]
dn_idx = [i for i, n in enumerate(neurons) if n["role"] in ("dna01", "dna02", "dnp09")]
assert sens_idx, "no mechanosensory partners in the extract"

def centroid(ids):
    return [sum(neurons[i]["pos"][k] for i in ids) / len(ids) for k in range(3)]

c_sens, c_dn = centroid(sens_idx), centroid(dn_idx)
strike_pos = [round((c_sens[k] + c_dn[k]) / 2, 3) for k in range(3)]
strike_pos[1] -= 1.5  # off the midline, never inside a real soma
strike_i = len(neurons)
neurons.append({"id": "authored:strike", "type": "authored", "role": "strike",
                "side": "center", "pos": strike_pos, "origin": "authored"})
authored_edges = []
print("authored: 1 neuron (the strike node, a vibration sense), 0 edges")

with open(os.path.join(OUT, "circuit.json"), "w", encoding="utf-8") as f:
    json.dump({"neurons": neurons, "edges": edges,
               "source": "chimera: every neuron with origin 'measured' and every edge with a fourth "
                         "column of 0 is a row of the fly's shipped FlyWire FAFB v783 extract "
                         "(data/circuit.json) minus the wing module; the 'authored' neuron and every "
                         "edge with a fourth column of 1 are invented and exist in no animal"}, f)
print(f"circuit.json: {len(neurons)} neurons, {len(edges)} edges -> {OUT}")

with open(os.path.join(OUT, "PROVENANCE.md"), "w", encoding="utf-8") as f:
    f.write(f"""# data/weaver — provenance

The one circuit shared by the three web-building chimeras — creatures that do
not exist (see `WEB_PLAN.md` §3). Three species, one wiring, because there is
no spider wiring to differ in.

| part | count | origin | licence |
|---|---|---|---|
| neurons, `origin: measured` | {len(neurons) - 1} | rows of the fly's shipped FlyWire FAFB v783 extract (`../circuit.json`), minus the wing module, copied by `etl_weaver.py` | CC BY-NC 4.0 (`../DATA_LICENSE.md`) |
| edges, fourth column `0` | {len(edges) - len(authored_edges)} | FlyWire FAFB v783 synapse counts, signed by neurotransmitter, as in the fly's extract | CC BY-NC 4.0 |
| neuron `authored:strike` | 1 | **authored** — no such neuron in any animal; a vibration sense with no synapses, driven by the modelled transduction | MIT (code licence) |
| edges, fourth column `1` | {len(authored_edges)} | — | — |

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
""")

role_of = {i: n["role"] for i, n in enumerate(neurons)}
def indeg(role):
    ids = {i for i, r in role_of.items() if r == role}
    return sum(abs(e[2]) for e in edges if e[1] in ids and e[3] == 0)
gf_ids = {i for i, r in role_of.items() if r == "gf"}
sens_gf = sum(abs(e[2]) for e in edges if e[0] in sens_idx and e[1] in gf_ids)
print(f"sanity: mechanosensory->GF {sens_gf:.0f} syn (measured; x gap-junction boost in the sim)")
for role in ("gf", "dna01", "dna02", "dnp09", "dng11", "mdn"):
    print(f"  in-circuit drive onto {role}: {indeg(role):.0f} syn")
print(f"  strike: no synapses in or out; driven by the vibration transduction only")
print("  roles:", dict(Counter(role_of.values())))
