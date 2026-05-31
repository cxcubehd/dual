const DEFAULT_INTERPOLATION_DELAY_MS: f64 = 100.0;

#[derive(Debug, Clone)]
pub struct InterpolationConfig {
    pub target_delay_ms: f64,
    pub min_buffer_snapshots: usize,
    pub max_buffer_snapshots: usize,
    pub time_correction_rate: f64,
    pub extrapolation_limit_ms: f64,
    pub snapshot_retention_ms: f64,
}

impl Default for InterpolationConfig {
    fn default() -> Self {
        Self {
            target_delay_ms: DEFAULT_INTERPOLATION_DELAY_MS,
            min_buffer_snapshots: 3,
            max_buffer_snapshots: 256,
            time_correction_rate: 0.1,
            extrapolation_limit_ms: 250.0,
            snapshot_retention_ms: 3000.0,
        }
    }
}
