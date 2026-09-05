# Data license

`brain_points.json` and `circuit.json` are derived from the publicly released
FlyWire connectome data products (FAFB v783), downloaded from
[FlyWire Codex](https://codex.flywire.ai) and processed by `../etl.py`.
`salticid/circuit.json` is derived from the same release by
`../etl_chimera.py`; every element marked `measured` in it is FlyWire data
under the terms below, and the one neuron and 127 edges marked `authored`
are invented and carry the code licence (MIT) — see `salticid/PROVENANCE.md`.

FlyWire data is licensed under
[CC BY-NC 4.0](https://creativecommons.org/licenses/by-nc/4.0/)
(Attribution-NonCommercial 4.0 International). These derived files are
likewise released under CC BY-NC 4.0: they may be shared and adapted with
attribution, for non-commercial use.

Please cite:

- Dorkenwald, S. et al. *Neuronal wiring diagram of an adult brain.*
  Nature 634, 124–138 (2024). https://doi.org/10.1038/s41586-024-07558-y
- Schlegel, P. et al. *Whole-brain annotation and multi-connectome cell typing
  of Drosophila.* Nature 634, 139–152 (2024).
  https://doi.org/10.1038/s41586-024-07686-5

FlyWire is a project of Princeton University and collaborators; see
https://flywire.ai for full terms and community guidelines.
