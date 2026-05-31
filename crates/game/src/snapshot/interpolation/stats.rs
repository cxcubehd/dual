#[derive(Debug, Clone)]
pub struct InterpolationStats {
    pub buffer_size: usize,
    pub render_time_ms: f64,
    pub server_time_offset_ms: f64,
    pub latest_server_tick: u32,
    pub entity_count: usize,
    pub is_ready: bool,
    pub is_extrapolating: bool,
    pub knowledge_tick: u32,
}
