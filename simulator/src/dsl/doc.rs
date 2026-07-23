//! Serde IR for FlyScenario TOML documents.

use serde::Deserialize;

/// Root FlyScenario document.
#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioDoc {
    /// Scenario meta block (required).
    pub scenario: MetaDoc,
    /// Virtual NICs.
    #[serde(default)]
    pub nic: Vec<NicDoc>,
    /// Pcap replay sources.
    #[serde(default)]
    pub pcap: Vec<PcapDoc>,
    /// Storage replay sources (parsed; wired when scheduler supports them).
    #[serde(default)]
    pub storage: Vec<StorageDoc>,
    /// Shared-memory fabric between sources and consumers.
    #[serde(default)]
    pub fabric: Option<FabricDoc>,
    /// Virtual consumers.
    #[serde(default)]
    pub consumer: Vec<ConsumerDoc>,
    /// Timed actions.
    #[serde(default)]
    pub timeline: Vec<TimelineDoc>,
    /// Educational controls.
    #[serde(default)]
    pub edu: Option<EduDoc>,
    /// Trace / metrics toggles.
    #[serde(default)]
    pub trace: Option<TraceDoc>,
    /// Optional Rhai script block (Phase 3).
    #[serde(default)]
    pub script: Option<ScriptDoc>,
}

/// `[scenario]` meta table.
#[derive(Debug, Clone, Deserialize)]
pub struct MetaDoc {
    /// Short identifier.
    pub name: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
    /// Run duration (`"5s"`, `"100ms"`, …).
    #[serde(default = "default_duration")]
    pub duration: String,
    /// Scheduler tick (`"1ms"`).
    #[serde(default = "default_tick")]
    pub tick: String,
    /// `"virtual"` or `"realtime"`.
    #[serde(default = "default_clock")]
    pub clock: String,
    /// `"benchmark"` | `"edu"` | `"trace"`.
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Determinism root seed.
    #[serde(default)]
    pub seed: u64,
    /// Must be `true` (or omitted); CLI refuses to hide simulation.
    #[serde(default = "default_true")]
    pub simulated: bool,
}

fn default_duration() -> String {
    "1s".into()
}
fn default_tick() -> String {
    "1ms".into()
}
fn default_clock() -> String {
    "virtual".into()
}
fn default_mode() -> String {
    "trace".into()
}
fn default_true() -> bool {
    true
}

/// `[[nic]]` source.
#[derive(Debug, Clone, Deserialize)]
pub struct NicDoc {
    /// Source name.
    pub name: String,
    /// Max packets per poll.
    #[serde(default = "default_batch")]
    pub batch_size: usize,
    /// UDP destination port written into synthetic frames.
    #[serde(default = "default_udp_port")]
    pub udp_port: u16,
    /// Traffic pattern.
    #[serde(default)]
    pub traffic: TrafficDoc,
    /// Payload generator.
    #[serde(default)]
    pub payload: PayloadDoc,
    /// Static faults.
    #[serde(default)]
    pub fault: FaultDoc,
}

fn default_batch() -> usize {
    64
}
fn default_udp_port() -> u16 {
    9000
}

/// Traffic sub-table.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TrafficDoc {
    /// `fixed` | `burst` | `gaussian` | `full`.
    #[serde(default = "default_pattern")]
    pub pattern: String,
    /// Packets/sec for `fixed`.
    #[serde(default)]
    pub pps: Option<u64>,
    /// Burst size for `burst`.
    #[serde(default)]
    pub burst_size: Option<usize>,
    /// Gap between bursts (`"1ms"`).
    #[serde(default)]
    pub gap: Option<String>,
    /// Mean pps for `gaussian`.
    #[serde(default)]
    pub mean_pps: Option<f64>,
    /// Stddev pps for `gaussian`.
    #[serde(default)]
    pub std_pps: Option<f64>,
    /// RNG seed for `gaussian`.
    #[serde(default)]
    pub seed: Option<u64>,
}

fn default_pattern() -> String {
    "fixed".into()
}

/// Payload sub-table.
#[derive(Debug, Clone, Deserialize)]
pub struct PayloadDoc {
    /// `fixed_seq` | `random` | `gaussian_size` | `protocol` | `custom`.
    #[serde(default = "default_payload_kind")]
    pub kind: String,
    /// Fixed / random payload size.
    #[serde(default)]
    pub size: Option<usize>,
    /// RNG seed.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Gaussian size mean.
    #[serde(default)]
    pub mean: Option<f64>,
    /// Gaussian size stddev (`std` in TOML).
    #[serde(default, rename = "std")]
    pub std_dev: Option<f64>,
    /// Gaussian size max.
    #[serde(default)]
    pub max: Option<usize>,
    /// Protocol name: `market_quote` | `fix_quote`.
    #[serde(default)]
    pub proto: Option<String>,
    /// Symbol for protocol payloads.
    #[serde(default)]
    pub symbol: Option<String>,
}

impl Default for PayloadDoc {
    fn default() -> Self {
        Self {
            kind: default_payload_kind(),
            size: Some(8),
            seed: None,
            mean: None,
            std_dev: None,
            max: None,
            proto: None,
            symbol: None,
        }
    }
}

fn default_payload_kind() -> String {
    "fixed_seq".into()
}

/// Fault sub-table.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FaultDoc {
    /// Drop probability `[0,1]`.
    #[serde(default)]
    pub drop_rate: f64,
    /// Corrupt probability `[0,1]`.
    #[serde(default)]
    pub corrupt_rate: f64,
    /// Latency spike probability `[0,1]`.
    #[serde(default)]
    pub latency_spike_rate: f64,
    /// Spike duration (`"500us"`).
    #[serde(default)]
    pub latency_spike: Option<String>,
    /// Fault LCG seed.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Reserved: malformed injection rate (future).
    #[serde(default)]
    pub malformed_rate: f64,
}

/// `[[pcap]]` source.
#[derive(Debug, Clone, Deserialize)]
pub struct PcapDoc {
    /// Source name.
    pub name: String,
    /// Path to classic pcap file.
    pub path: String,
    /// `full` | `original` | `scaled` | `burst` | `single_step`.
    #[serde(default = "default_replay")]
    pub replay: String,
    /// Scale factor for `scaled`.
    #[serde(default = "default_scale")]
    pub scale: f64,
    /// Loop capture when exhausted.
    #[serde(default)]
    pub r#loop: bool,
    /// Optional faults.
    #[serde(default)]
    pub fault: FaultDoc,
}

fn default_replay() -> String {
    "full".into()
}
fn default_scale() -> f64 {
    1.0
}

/// `[[storage]]` source (IR only for now).
#[derive(Debug, Clone, Deserialize)]
pub struct StorageDoc {
    /// Source name.
    pub name: String,
    /// Input path.
    pub path: String,
    /// `delimiter` | `fixed` | `length_prefixed`.
    #[serde(default = "default_frame")]
    pub frame: String,
    /// Replay mode name.
    #[serde(default = "default_replay")]
    pub replay: String,
}

fn default_frame() -> String {
    "delimiter".into()
}

/// `[fabric]` ring.
#[derive(Debug, Clone, Deserialize)]
pub struct FabricDoc {
    /// Ring name.
    #[serde(default = "default_ring_name")]
    pub name: String,
    /// Slot count.
    #[serde(default = "default_slots")]
    pub slots: usize,
    /// Max frame bytes per slot.
    #[serde(default)]
    pub max_frame: Option<usize>,
}

fn default_ring_name() -> String {
    "ring0".into()
}
fn default_slots() -> usize {
    4096
}

/// `[[consumer]]`.
#[derive(Debug, Clone, Deserialize)]
pub struct ConsumerDoc {
    /// Consumer name.
    pub name: String,
    /// Max slots per drain (`number` or `"unlimited"`).
    #[serde(default)]
    pub max_per_drain: Option<toml::Value>,
    /// Reserved future virtual-time stall.
    #[serde(default)]
    pub drain_delay: Option<String>,
}

/// `[[timeline]]` entry.
#[derive(Debug, Clone, Deserialize)]
pub struct TimelineDoc {
    /// Fire time (`"200ms"`).
    pub at: String,
    /// `set_traffic` | `set_fault` | `slow_consumer`.
    pub action: String,
    /// Target NIC (for traffic/fault).
    #[serde(default)]
    pub nic: Option<String>,
    /// Target consumer.
    #[serde(default)]
    pub consumer: Option<String>,
    /// Traffic pattern name override (`fixed` / `burst` / …).
    #[serde(default)]
    pub pattern: Option<String>,
    /// Fixed-rate pps override.
    #[serde(default)]
    pub pps: Option<u64>,
    /// Burst size override.
    #[serde(default)]
    pub burst_size: Option<usize>,
    /// Burst gap override (`"1ms"`).
    #[serde(default)]
    pub gap: Option<String>,
    /// Gaussian mean pps override.
    #[serde(default)]
    pub mean_pps: Option<f64>,
    /// Gaussian stddev pps override.
    #[serde(default)]
    pub std_pps: Option<f64>,
    /// Pattern / fault seed override.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Drop-rate override for `set_fault`.
    #[serde(default)]
    pub drop_rate: Option<f64>,
    /// Corrupt-rate override for `set_fault`.
    #[serde(default)]
    pub corrupt_rate: Option<f64>,
    /// Latency-spike-rate override for `set_fault`.
    #[serde(default)]
    pub latency_spike_rate: Option<f64>,
    /// Latency spike duration override (`"500us"`).
    #[serde(default)]
    pub latency_spike: Option<String>,
    /// Consumer budget.
    #[serde(default)]
    pub max_per_drain: Option<toml::Value>,
}

/// `[edu]` controls.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EduDoc {
    /// Start paused (drive via step / TUI).
    #[serde(default)]
    pub paused_start: bool,
    /// Break after this many ticks.
    #[serde(default)]
    pub breakpoint_tick: Option<u64>,
    /// Break at virtual time.
    #[serde(default)]
    pub breakpoint_ns: Option<String>,
}

/// `[trace]` toggles.
#[derive(Debug, Clone, Deserialize)]
pub struct TraceDoc {
    /// Emit structured events.
    #[serde(default = "default_true")]
    pub events: bool,
    /// Record metrics.
    #[serde(default = "default_true")]
    pub metrics: bool,
}

impl Default for TraceDoc {
    fn default() -> Self {
        Self {
            events: true,
            metrics: true,
        }
    }
}

/// `[script]` Rhai block.
#[derive(Debug, Clone, Deserialize)]
pub struct ScriptDoc {
    /// Must be `"rhai"`.
    #[serde(default = "default_engine")]
    pub engine: String,
    /// Inline source.
    #[serde(default)]
    pub source: Option<String>,
    /// Path to a `.rhai` file (relative to the scenario file).
    #[serde(default)]
    pub path: Option<String>,
}

fn default_engine() -> String {
    "rhai".into()
}
