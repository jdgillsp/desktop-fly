//! Loading the shipped connectome data.
//!
//! Direct port of `Sim.swift:33-58`. The JSON files are unchanged from the
//! macOS build — `data/` is FlyWire-derived and CC BY-NC 4.0, and the
//! code/data licence split must stay intact (PORT_PLAN.md §3).

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// 23,210 real soma positions for the brain window, plus a super-class index.
#[derive(Debug, Clone, Deserialize)]
pub struct BrainPointsFile {
    pub classes: Vec<String>,
    /// `[x, y, z, class_index]`
    pub points: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CircuitNeuron {
    pub id: String,
    /// FlyWire `primary_type` for core neurons; `super_class` for partners.
    #[serde(rename = "type")]
    pub cell_type: String,
    /// Role slug: lc4 | lplc2 | gf | dna01 | dna02 | dnp09 | dng11 | mdn | escw | other
    pub role: String,
    /// left | right | center
    pub side: String,
    pub pos: Vec<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CircuitFile {
    pub neurons: Vec<CircuitNeuron>,
    /// `[pre_index, post_index, signed_synapse_count]`
    pub edges: Vec<Vec<f32>>,
    /// `[a_index, b_index, conductance]` gap junctions. Absent from the fly's
    /// file (FlyWire is chemical-only); written by `etl_celegans.py`, where
    /// it is a third of the graph.
    #[serde(default)]
    pub electrical: Vec<Vec<f32>>,
}

#[derive(Debug, Clone)]
pub struct BrainData {
    pub points: BrainPointsFile,
    pub circuit: CircuitFile,
}

/// Port of `findDataDir()` (Sim.swift:39). Looks next to the executable and in
/// the working directory, then walks up — the Rust build puts binaries in
/// `rust/target/release`, several levels below the repo's `data/`.
pub fn find_data_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("data"));
            let mut up = dir;
            for _ in 0..5 {
                match up.parent() {
                    Some(p) => {
                        candidates.push(p.join("data"));
                        up = p;
                    }
                    None => break,
                }
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("data"));
        let mut up = cwd.as_path();
        for _ in 0..5 {
            match up.parent() {
                Some(p) => {
                    candidates.push(p.join("data"));
                    up = p;
                }
                None => break,
            }
        }
    }
    if let Ok(explicit) = std::env::var("DESKTOPFLY_DATA") {
        candidates.insert(0, PathBuf::from(explicit));
    }

    candidates
        .into_iter()
        .find(|d| d.join("circuit.json").is_file())
}

pub fn load_from_dir(dir: &Path) -> Result<BrainData, String> {
    let read = |name: &str| -> Result<String, String> {
        std::fs::read_to_string(dir.join(name))
            .map_err(|e| format!("{}: {e}", dir.join(name).display()))
    };
    let points: BrainPointsFile =
        serde_json::from_str(&read("brain_points.json")?).map_err(|e| format!("brain_points.json: {e}"))?;
    let circuit: CircuitFile =
        serde_json::from_str(&read("circuit.json")?).map_err(|e| format!("circuit.json: {e}"))?;
    Ok(BrainData { points, circuit })
}

pub fn load() -> Result<BrainData, String> {
    let dir = find_data_dir()
        .ok_or_else(|| "no data/ directory found — run etl.py, or set DESKTOPFLY_DATA".to_string())?;
    load_from_dir(&dir)
}

/// Load a creature's data by its `Creature::data_dir()`, relative to the
/// shipped `data/` directory. `"."` is the fly; `"c_elegans"` the worm.
///
/// The fly's files anchor `find_data_dir`, so a second creature's directory is
/// always looked up beside them rather than searched for on its own — one
/// data root, several animals.
pub fn load_for(data_dir: &str) -> Result<BrainData, String> {
    let root = find_data_dir()
        .ok_or_else(|| "no data/ directory found — run etl.py, or set DESKTOPFLY_DATA".to_string())?;
    let dir = if data_dir == "." {
        root
    } else {
        root.join(data_dir)
    };
    if !dir.join("circuit.json").is_file() {
        return Err(format!(
            "no connectome under {} — this creature's data is not shipped; see its ETL script",
            dir.display()
        ));
    }
    load_from_dir(&dir)
}
