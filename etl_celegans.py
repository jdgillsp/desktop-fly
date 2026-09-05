#!/usr/bin/env python3
"""Build DesktopFly's C. elegans connectome from a published edge list.

Companion to etl.py, which does the same job for FlyWire. Two differences
matter, and both come from the animal rather than from convenience:

  1. C. elegans has TWO connectomes, not one. Chemical synapses are directional
     and signed; gap junctions are electrical, symmetric and unsigned, and they
     are roughly a third of the graph. They are emitted as separate edge lists
     because the simulation treats them differently -- see rust/core/src/graded.rs.

  2. The neurons are individually named and individually characterised, so the
     role assignment below is by NAME, not by a cell-type cluster. Every neuron
     in the locomotor and mechanosensory circuits is listed explicitly.

Outputs (into data/c_elegans/):
  brain_points.json   soma positions for the brain window
  circuit.json        neurons + chemical edges + electrical edges

--------------------------------------------------------------------------
DATA IS NOT SHIPPED WITH THIS REPO.

PORT_PLAN.md sec 8 flags the redistribution terms for the C. elegans datasets
as unverified, and rust/core/src/creature.rs records that in the creature's
Provenance rather than quietly implying clearance. Check the licence of
whichever source you use before committing anything under data/c_elegans/,
and keep the code/data licence split intact the way data/DATA_LICENSE.md does
for FlyWire.

Commonly used sources (verify terms yourself):
  - WormWiring (Emmons lab) -- adjacency matrices for both sexes
  - Cook et al. 2019, Nature 571:63-71 -- supplementary data
  - OpenWorm / c302 -- CC-BY connectome derivatives
  - Varshney et al. 2011, PLoS Comput Biol 7:e1001066 -- classic edge list

Usage: python3 etl_celegans.py <edges.csv> [positions.csv]

  edges.csv     pre,post,type,weight   where type is "chemical" or "electrical"
  positions.csv neuron,x,y,z           optional; a ring layout is synthesised
                                       if absent, and the brain window says so
--------------------------------------------------------------------------
"""
import csv
import json
import math
import os
import sys

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data", "c_elegans")

# Neuron -> role slug. Must match the population slugs in
# rust/core/src/roles.rs::c_elegans(), or the manifest resolves empty groups
# and the worm does nothing.
#
# Grouped by function, following the standard locomotor circuit description.
ROLES = {}


def _assign(role, names):
    for n in names:
        ROLES[n] = role


# Command interneurons: direction.
_assign("forward", ["AVBL", "AVBR", "PVCL", "PVCR"])
_assign("reverse", ["AVAL", "AVAR", "AVDL", "AVDR", "AVEL", "AVER"])

# Motor neuron classes: execution. VB/DB drive forward, VA/DA backward,
# VD/DD are inhibitory and shape the wave.
_assign("motor_b", [f"VB{i:02d}" for i in range(1, 12)] + [f"DB{i:02d}" for i in range(1, 8)])
_assign("motor_a", [f"VA{i:02d}" for i in range(1, 13)] + [f"DA{i:02d}" for i in range(1, 10)])
_assign("motor_d", [f"VD{i:02d}" for i in range(1, 14)] + [f"DD{i:02d}" for i in range(1, 7)])

# Mechanosensation: the tap-withdrawal circuit. Anterior touch triggers
# reversal, posterior touch accelerates forward -- opposite behaviours, so they
# are separate populations.
_assign("touch", ["ALML", "ALMR", "AVM"])
_assign("touch_post", ["PLML", "PLMR", "PVM"])

# AFD is the thermosensor: the worm migrates toward its cultivation
# temperature, which is what makes machine heat a meaningful stimulus.
_assign("thermo", ["AFDL", "AFDR"])
_assign("chemo", ["AWAL", "AWAR", "AWCL", "AWCR"])

# Omega turn, and the sleep/quiescence gate.
_assign("turn", ["RIML", "RIMR", "RIVL", "RIVR", "SMDDL", "SMDDR", "SMDVL", "SMDVR"])
_assign("sleep", ["RIS"])


def side_of(name):
    """L/R suffix, where the animal is bilaterally symmetric."""
    if name.endswith("L"):
        return "left"
    if name.endswith("R"):
        return "right"
    return "center"


def role_of(name):
    if name in ROLES:
        return ROLES[name]
    # Anything unlisted is still simulated, just not read out by name.
    # Sensory neurons are conventionally named with a leading A/B/I/O.
    return "inter"


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    edges_path = sys.argv[1]
    pos_path = sys.argv[2] if len(sys.argv) > 2 else None
    os.makedirs(OUT, exist_ok=True)

    chemical, electrical = [], []
    names = []
    index = {}

    def idx(name):
        if name not in index:
            index[name] = len(names)
            names.append(name)
        return index[name]

    with open(edges_path, newline="") as f:
        reader = csv.reader(f)
        header = next(reader)
        print("edge header:", header)
        for row in reader:
            if len(row) < 4:
                continue
            pre, post, kind, weight = row[0].strip(), row[1].strip(), row[2].strip().lower(), row[3]
            try:
                w = float(weight)
            except ValueError:
                continue
            i, j = idx(pre), idx(post)
            if kind.startswith("e") or "gap" in kind or "junction" in kind:
                electrical.append([i, j, abs(w)])
            else:
                # Sign by transmitter where the source provides it; otherwise
                # treat GABAergic D-class motor neurons as inhibitory, which is
                # the one well-established sign in this circuit.
                sign = -1.0 if role_of(pre) == "motor_d" else 1.0
                chemical.append([i, j, round(sign * abs(w), 2)])

    print(f"neurons: {len(names)}  chemical: {len(chemical)}  electrical: {len(electrical)}")
    if not names:
        sys.exit("no neurons parsed -- check the edge file's columns")

    positions = {}
    if pos_path:
        with open(pos_path, newline="") as f:
            reader = csv.reader(f)
            next(reader)
            for row in reader:
                if len(row) >= 4:
                    positions[row[0].strip()] = (float(row[1]), float(row[2]), float(row[3]))
        print(f"positions: {len(positions)}")

    # Normalise into [-10, 10], matching what etl.py does for the fly so the
    # brain window needs no per-creature scaling.
    pts = []
    if positions:
        xs = [p[0] for p in positions.values()]
        ys = [p[1] for p in positions.values()]
        zs = [p[2] for p in positions.values()]
        cx, cy, cz = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2, (min(zs) + max(zs)) / 2
        span = max(max(xs) - min(xs), max(ys) - min(ys), max(zs) - min(zs)) or 1.0
        scale = 20.0 / span

        def norm(p):
            return [round((p[0] - cx) * scale, 3),
                    round((p[1] - cy) * scale, 3),
                    round((p[2] - cz) * scale, 3)]
    else:
        # No coordinates: lay the animal out as a body-ordered helix. Honest,
        # because the brain window labels this layout as synthetic rather than
        # pretending it is anatomy.
        print("no positions supplied -- synthesising a body-ordered layout")

        def norm(_p):
            return [0.0, 0.0, 0.0]

    neurons = []
    for k, name in enumerate(names):
        if name in positions:
            p = norm(positions[name])
        else:
            t = k / max(1, len(names) - 1)
            angle = t * math.tau * 2.0
            p = [round(math.cos(angle) * 3.0, 3),
                 round(-10.0 + t * 20.0, 3),
                 round(math.sin(angle) * 3.0, 3)]
        neurons.append({
            "id": name,
            "type": name,
            "role": role_of(name),
            "side": side_of(name),
            "pos": p,
        })
        pts.append(p + [0])

    with open(os.path.join(OUT, "circuit.json"), "w") as f:
        json.dump({
            "neurons": neurons,
            "edges": chemical,
            "electrical": electrical,
            "source": f"C. elegans connectome from {os.path.basename(edges_path)}",
            "layout": "anatomical" if positions else "synthesised body-ordered",
        }, f)
    with open(os.path.join(OUT, "brain_points.json"), "w") as f:
        json.dump({"classes": ["neuron"], "points": pts,
                   "source": "C. elegans soma positions"}, f)

    covered = sum(1 for n in neurons if n["role"] != "inter")
    print(f"circuit.json: {len(neurons)} neurons, {len(chemical)} chemical, "
          f"{len(electrical)} electrical")
    print(f"named populations cover {covered} neurons")
    missing = [k for k in ROLES if k not in index]
    if missing:
        print(f"WARNING: {len(missing)} expected neurons absent from the edge list, "
              f"e.g. {missing[:8]}")
        print("The locomotor readout will be weaker than intended.")


if __name__ == "__main__":
    main()
