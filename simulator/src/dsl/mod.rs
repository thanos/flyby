//! FlyScenario DSL: TOML scenario documents compiled onto the simulator.
//!
//! User-facing language reference: `docs/src/scenario-dsl.md`.
//!
//! ```text
//! *.fly.toml  →  ScenarioDoc  →  CompiledRun  →  SimScheduler
//!                     ↑
//!              optional [script] (Rhai) → timeline actions
//! ```

mod compile;
mod doc;
mod duration;
mod script;

pub use compile::{CompiledConsumer, CompiledFabric, CompiledNic, CompiledPcap, CompiledRun};
pub use doc::{
    ConsumerDoc, EduDoc, FabricDoc, FaultDoc, MetaDoc, NicDoc, PayloadDoc, PcapDoc, ScenarioDoc,
    ScriptDoc, StorageDoc, TimelineDoc, TraceDoc, TrafficDoc,
};
pub use duration::parse_duration_ns;
pub use script::eval_script;

use std::path::Path;

use flyby_core::{Error, Result};

/// Parse a FlyScenario TOML document from a string.
pub fn parse_str(toml_src: &str) -> Result<ScenarioDoc> {
    toml::from_str(toml_src).map_err(|e| Error::config(format!("FlyScenario TOML: {e}")))
}

/// Load and parse a FlyScenario file (`.fly.toml` / `.toml`).
pub fn load_path(path: impl AsRef<Path>) -> Result<ScenarioDoc> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|e| {
        Error::config(format!(
            "failed to read scenario file '{}': {e}",
            path.display()
        ))
    })?;
    parse_str(&text)
}

/// Load, evaluate optional Rhai script, and compile to a runnable plan.
pub fn compile_path(path: impl AsRef<Path>) -> Result<CompiledRun> {
    let path = path.as_ref();
    let doc = load_path(path)?;
    let base = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    compile::compile_doc(doc, &base)
}

/// Parse + compile from an in-memory TOML string.
///
/// `base_dir` resolves relative pcap/storage/script paths.
pub fn compile_str(toml_src: &str, base_dir: impl AsRef<Path>) -> Result<CompiledRun> {
    let doc = parse_str(toml_src)?;
    compile::compile_doc(doc, base_dir.as_ref())
}

/// `true` when `arg` looks like a scenario file path rather than a built-in name.
pub fn looks_like_scenario_file(arg: &str) -> bool {
    let lower = arg.to_ascii_lowercase();
    lower.ends_with(".fly.toml")
        || lower.ends_with(".toml")
        || arg.contains('/')
        || arg.contains('\\')
}
