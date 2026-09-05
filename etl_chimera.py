#!/usr/bin/env python3
"""Build the jumping-spider chimera's circuit from the FlyWire Codex v783 dumps.

The chimera (SPIDER_PLAN.md) is an animal that does not exist, assembled from
circuit modules that do. This script is etl.py with three deliberate
differences, and it writes to data/salticid/ so the fly's shipped files -- the
oracle the Rust port is checked against -- are never touched:

  1. Core populations: the fly's minus the wing module (DNp02/04/11 -- a spider
     has no wings), plus LC11, FlyWire's small-object motion detectors, which
     give the animal a real prey-detection input.
  2. Partners: as etl.py, plus LC11's strongest DOWNSTREAM partners, so
     whatever the real LC11 axons drive inside the extract is present and the
     measured pathway is as deep as the data allows.
  3. One AUTHORED neuron -- the pounce node -- with authored edges from every
     LC11 cell. No animal has it. It is the one connective in the circuit that
     is invented, and it is labelled as such per neuron and per edge:
     neurons carry "origin": "measured" | "authored", and edge rows are
     [pre, post, weight, origin] with origin 1 = authored.

Every measured element is CC BY-NC 4.0 FlyWire data exactly as etl.py extracts
it. See data/salticid/PROVENANCE.md.

Usage: python3 etl_chimera.py <raw_dir>
"""
import csv, gzip, json, os, sys
from collections import Counter, defaultdict

RAW = sys.argv[1] if len(sys.argv) > 1 else "."
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "data", "salticid")
os.makedirs(OUT, exist_ok=True)

CORE_TYPES = {          # primary_type -> role
    "LC4": "lc4",       # looming detector population (giant-fiber input)
    "LPLC2": "lplc2",   # looming detector population (giant-fiber input)
    "LC11": "lc11",     # small-object motion detectors (prey)        <- NEW
    "DNp01": "gf",      # giant fiber (escape command neuron)
    "DNa02": "dna02",   # steering descending neuron
    "DNa01": "dna01",   # steering descending neuron (partner of DNa02)
    "DNp09": "dnp09",   # forward-walking command neuron
    "DNg11": "dng11",   # grooming command neuron
    "MDN": "mdn",       # moonwalker (backward walking) descending neuron
    # DNp02 / DNp04 / DNp11 (escw): dropped -- no wings.
}
NT_SIGN = {"ACH": 1.0, "GABA": -1.0, "GLUT": -1.0, "DA": 0.5, "SER": 0.5, "OCT": 0.5}
MAX_PARTNERS = 330
LC11_DOWNSTREAM = 20

# The one authored parameter. Per-synapse weight from each LC11 cell onto the
# pounce node, in the same synapse-count units as the measured edges, so the
# simulator scales it identically (weight_scale 0.0008). Chosen so the node is
# silent at LC11's resting rate and fires under sustained small-object drive;
# rust/core/src/suites.rs::chimera_test is the check.
POUNCE_WEIGHT = 8.0

def rows(name):
    with gzip.open(os.path.join(RAW, name), "rt") as f:
        r = csv.reader(f)
        next(r)
        yield from r

# --- cell types ------------------------------------------------------------
core, type_of, counts = {}, {}, defaultdict(int)
for row in rows("consolidated_cell_types.csv.gz"):
    rid, ptype = row[0], row[1].strip()
    role = CORE_TYPES.get(ptype)
    if role:
        core[rid] = role
        type_of[rid] = ptype
        counts[ptype] += 1
print("core populations:", dict(counts))
for must in ("LC4", "LPLC2", "LC11", "DNp01"):
    if not counts.get(must):
        sys.exit(f"FATAL: missing core population {must}")

# --- classification + coordinates ------------------------------------------
klass = {}
for row in rows("classification.csv.gz"):
    klass[row[0]] = (row[2], row[6])
pos = {}
for row in rows("coordinates.csv.gz"):
    rid = row[0]
    if rid in pos:
        continue
    p = row[1].strip("[]").split()
    if len(p) == 3:
        pos[rid] = (float(p[0]), float(p[1]), float(p[2]))

# --- connections pass 1: partner strengths ---------------------------------
partner_strength = defaultdict(int)
strength_by_role = defaultdict(lambda: defaultdict(int))
downstream_of_role = defaultdict(lambda: defaultdict(int))
for row in rows("connections.csv.gz"):
    pre, post, syn = row[0], row[1], int(row[3])
    pre_core, post_core = pre in core, post in core
    if pre_core and not post_core:
        partner_strength[post] += syn
        strength_by_role[core[pre]][post] += syn
        downstream_of_role[core[pre]][post] += syn
    elif post_core and not pre_core:
        partner_strength[pre] += syn
        strength_by_role[core[post]][pre] += syn

usable = lambda r: r in pos and r in klass
ranked = [rid for rid, s in sorted(partner_strength.items(), key=lambda kv: -kv[1]) if usable(rid)]
partners, seen = [], set()
def take(cands, k):
    n = 0
    for r in cands:
        if r in seen or not usable(r):
            continue
        seen.add(r); partners.append(r); n += 1
        if n == k:
            break
for role in ("gf", "dna01", "dna02", "dnp09", "dng11", "mdn"):
    take([r for r, s in sorted(strength_by_role[role].items(), key=lambda kv: -kv[1])], 10)
# LC11's real targets, so the prey pathway is measured as far as it goes.
take([r for r, s in sorted(downstream_of_role["lc11"].items(), key=lambda kv: -kv[1])], LC11_DOWNSTREAM)
take([r for r in ranked if klass[r][0] == "ascending"], 24)
take([r for r in ranked if klass[r][0] == "sensory"], 16)
take(ranked, MAX_PARTNERS - len(partners))
print("partner super_classes:", dict(Counter(klass[r][0] for r in partners)))
members = list(core.keys()) + partners
member_idx = {rid: i for i, rid in enumerate(members)}
print(f"measured members: {len(members)} ({len(core)} core + {len(partners)} partners)")

# --- connections pass 2: edges within the circuit --------------------------
edges = []
for row in rows("connections.csv.gz"):
    pre, post = row[0], row[1]
    i, j = member_idx.get(pre), member_idx.get(post)
    if i is None or j is None:
        continue
    syn, nt = int(row[3]), row[4].strip().upper()
    sign = NT_SIGN.get(nt, 1.0)
    edges.append([i, j, round(syn * sign, 1), 0])
print(f"measured edges: {len(edges)}")

# --- normalisation, identical to etl.py so the soma cloud is shared --------
xs = [p[0] for p in pos.values()]; ys = [p[1] for p in pos.values()]; zs = [p[2] for p in pos.values()]
cx, cy, cz = (min(xs)+max(xs))/2, (min(ys)+max(ys))/2, (min(zs)+max(zs))/2
scale = 20.0 / max(max(xs)-min(xs), max(ys)-min(ys), max(zs)-min(zs))
def norm(p):
    return (round((p[0]-cx)*scale, 3), round(-(p[1]-cy)*scale, 3), round(-(p[2]-cz)*scale, 3))

neurons = []
for rid in members:
    sc, side = klass.get(rid, ("", ""))
    p = norm(pos[rid]) if rid in pos else (0, 0, 0)
    neurons.append({"id": rid, "type": type_of.get(rid, sc or "?"),
                    "role": core.get(rid, "other"), "side": side, "pos": list(p),
                    "origin": "measured"})

# --- the authored pounce node ------------------------------------------------
lc11_idx = [i for i, n in enumerate(neurons) if n["role"] == "lc11"]
dn_idx = [i for i, n in enumerate(neurons) if n["role"] in ("dna01", "dna02", "dnp09")]
def centroid(ids):
    return [sum(neurons[i]["pos"][k] for i in ids) / len(ids) for k in range(3)]
c_lc11, c_dn = centroid(lc11_idx), centroid(dn_idx)
# Placed between the population it reads and the descending neurons it would
# have to talk to, and off the midline so it never hides inside a real soma.
pounce_pos = [round((c_lc11[k] + c_dn[k]) / 2, 3) for k in range(3)]
pounce_pos[1] += 1.5
pounce_i = len(neurons)
neurons.append({"id": "authored:pounce", "type": "authored", "role": "pounce",
                "side": "center", "pos": pounce_pos, "origin": "authored"})
authored_edges = [[i, pounce_i, POUNCE_WEIGHT, 1] for i in lc11_idx]
edges.extend(authored_edges)
print(f"authored: 1 neuron, {len(authored_edges)} edges (LC11 -> pounce, {POUNCE_WEIGHT} each)")

with open(os.path.join(OUT, "circuit.json"), "w", encoding="utf-8") as f:
    json.dump({"neurons": neurons, "edges": edges,
               "source": "chimera: FlyWire Codex FAFB v783 connections.csv (syn>=5, signed by nt_type) "
                         "for every neuron with origin 'measured'; the 'authored' neuron and every "
                         "edge with a fourth column of 1 are invented and exist in no animal"}, f)
print(f"circuit.json: {len(neurons)} neurons, {len(edges)} edges -> {OUT}")

with open(os.path.join(OUT, "PROVENANCE.md"), "w", encoding="utf-8") as f:
    f.write(f"""# data/salticid — provenance

This is the circuit of a creature that does not exist (see `SPIDER_PLAN.md`).

| part | count | origin | licence |
|---|---|---|---|
| neurons, `origin: measured` | {len(neurons) - 1} | FlyWire FAFB v783, extracted by `etl_chimera.py` exactly as `etl.py` extracts the fly's | CC BY-NC 4.0 (`../DATA_LICENSE.md`) |
| edges, fourth column `0` | {len(edges) - len(authored_edges)} | FlyWire FAFB v783 synapse counts, signed by neurotransmitter | CC BY-NC 4.0 |
| neuron `authored:pounce` | 1 | **authored** — no such neuron in any animal | MIT (code licence) |
| edges, fourth column `1` | {len(authored_edges)} | **authored** — LC11 → pounce, weight {POUNCE_WEIGHT} each | MIT |

`brain_points.json` is not duplicated here; the loader falls back to the fly's,
because the measured modules *are* the fly's and their somas sit where they sit.

The measured modules: LC4/LPLC2 (looming), DNp01 (giant fiber), DNa01/DNa02
(steering), DNp09 (walking), DNg11 (grooming), MDN (backing up), and LC11
(small-object detection) with its strongest downstream partners. The fly's
wing module (DNp02/04/11) is omitted.
""")

# --- report ------------------------------------------------------------------
role_of = {i: n["role"] for i, n in enumerate(neurons)}
def indeg(role):
    ids = {i for i, r in role_of.items() if r == role}
    return sum(abs(e[2]) for e in edges if e[1] in ids and e[3] == 0)
def outdeg(role):
    ids = {i for i, r in role_of.items() if r == role}
    return sum(abs(e[2]) for e in edges if e[0] in ids and e[3] == 0)
gf_ids = {i for i, r in role_of.items() if r == "gf"}
loom_gf = sum(abs(e[2]) for e in edges if role_of[e[0]] in ("lc4", "lplc2") and e[1] in gf_ids)
print(f"sanity: loom->GF syn {loom_gf:.0f}")
for role in ("gf", "dna01", "dna02", "dnp09", "dng11", "mdn"):
    print(f"  in-circuit drive onto {role}: {indeg(role):.0f} syn")
print(f"  LC11: in-circuit drive onto it {indeg('lc11'):.0f} syn (sensory input population; "
      f"driven by transduction like LC4), measured out-drive from it {outdeg('lc11'):.0f} syn")
lc11_to = Counter(role_of[e[1]] for e in edges if role_of[e[0]] == "lc11" and e[3] == 0)
print(f"  LC11 measured targets by role: {dict(lc11_to)}")
