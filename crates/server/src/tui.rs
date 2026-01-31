use std::collections::VecDeque;
use std::time::Instant;

use dual::PacketLossSimulation;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Table, Row, Cell};

use crate::server::ServerStats;

const MAX_LOG_ENTRIES: usize = 1000;
const VISIBLE_LOG_LINES: usize = 20;

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: Instant,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn color(&self) -> Color {
        match self {
            LogLevel::Info => Color::White,
            LogLevel::Warn => Color::Yellow,
            LogLevel::Error => Color::Red,
        }
    }

    fn prefix(&self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERR ",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Console,
    Connections,
}

impl Tab {
    fn all() -> &'static [Tab] {
        &[Tab::Console, Tab::Connections]
    }

    fn title(&self) -> &'static str {
        match self {
            Tab::Console => "Console",
            Tab::Connections => "Connections",
        }
    }

    fn index(&self) -> usize {
        match self {
            Tab::Console => 0,
            Tab::Connections => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketLossField {
    Direction,
    Enabled,
    LossPercent,
    MinLatency,
    MaxLatency,
    Jitter,
}

impl PacketLossField {
    fn all() -> &'static [Self] {
        &[
            Self::Direction,
            Self::Enabled,
            Self::LossPercent,
            Self::MinLatency,
            Self::MaxLatency,
            Self::Jitter,
        ]
    }

    fn next(&self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|f| f == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    fn prev(&self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|f| f == self).unwrap_or(0);
        all[(idx + all.len() - 1) % all.len()]
    }
}

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub client_id: u32,
    pub addr: String,
    pub entity_id: Option<u32>,
    pub connected_secs: u64,
    pub last_ping_ms: f32,
    pub packet_loss_sim: PacketLossSimulation,
    pub incoming_packet_loss_sim: PacketLossSimulation,
}

pub struct TuiState {
    logs: VecDeque<LogEntry>,
    scroll_offset: usize,
    start_time: Instant,
    active_tab: Tab,
    selected_connection: usize,
    pending_kick: Option<u32>,
    pub packet_loss_panel: Option<PacketLossPanelState>,
    pub pending_packet_loss_update: Option<(u32, PacketLossSimulation, PacketLossSimulation)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketDirection {
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone)]
pub struct PacketLossPanelState {
    pub client_id: u32,
    pub selected_field: PacketLossField,
    pub sim: PacketLossSimulation,
    pub incoming_sim: PacketLossSimulation,
    pub direction: PacketDirection,
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            logs: VecDeque::with_capacity(MAX_LOG_ENTRIES),
            scroll_offset: 0,
            start_time: Instant::now(),
            active_tab: Tab::Console,
            selected_connection: 0,
            pending_kick: None,
            packet_loss_panel: None,
            pending_packet_loss_update: None,
        }
    }

    pub fn log(&mut self, level: LogLevel, message: String) {
        if self.logs.len() >= MAX_LOG_ENTRIES {
            self.logs.pop_front();
            if self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
        }
        self.logs.push_back(LogEntry {
            timestamp: Instant::now(),
            level,
            message,
        });
    }

    pub fn log_info(&mut self, message: impl Into<String>) {
        self.log(LogLevel::Info, message.into());
    }

    pub fn log_warn(&mut self, message: impl Into<String>) {
        self.log(LogLevel::Warn, message.into());
    }

    pub fn log_error(&mut self, message: impl Into<String>) {
        self.log(LogLevel::Error, message.into());
    }

    pub fn scroll_up(&mut self) {
        let max_scroll = self.logs.len().saturating_sub(VISIBLE_LOG_LINES);
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn next_tab(&mut self) {
        self.active_tab = match self.active_tab {
            Tab::Console => Tab::Connections,
            Tab::Connections => Tab::Console,
        };
    }

    pub fn prev_tab(&mut self) {
        self.next_tab();
    }

    pub fn select_next_connection(&mut self, max: usize) {
        if max > 0 {
            self.selected_connection = (self.selected_connection + 1) % max;
        }
    }

    pub fn select_prev_connection(&mut self, max: usize) {
        if max > 0 {
            self.selected_connection = self.selected_connection.checked_sub(1).unwrap_or(max - 1);
        }
    }

    pub fn request_kick(&mut self, clients: &[ClientInfo]) {
        if let Some(client) = clients.get(self.selected_connection) {
            self.pending_kick = Some(client.client_id);
        }
    }

    pub fn take_pending_kick(&mut self) -> Option<u32> {
        self.pending_kick.take()
    }

    pub fn active_tab(&self) -> Tab {
        self.active_tab
    }

    pub fn is_packet_loss_panel_open(&self) -> bool {
        self.packet_loss_panel.is_some()
    }

    pub fn open_packet_loss_panel(&mut self, clients: &[ClientInfo]) {
        if let Some(client) = clients.get(self.selected_connection) {
            self.packet_loss_panel = Some(PacketLossPanelState {
                client_id: client.client_id,
                selected_field: PacketLossField::Direction,
                sim: client.packet_loss_sim.clone(),
                incoming_sim: client.incoming_packet_loss_sim.clone(),
                direction: PacketDirection::Outgoing,
            });
        }
    }

    pub fn close_packet_loss_panel(&mut self) {
        if let Some(panel) = self.packet_loss_panel.take() {
            self.pending_packet_loss_update = Some((panel.client_id, panel.sim, panel.incoming_sim));
        }
    }

    pub fn cancel_packet_loss_panel(&mut self) {
        self.packet_loss_panel = None;
    }

    pub fn packet_loss_panel_next_field(&mut self) {
        if let Some(panel) = &mut self.packet_loss_panel {
            panel.selected_field = panel.selected_field.next();
        }
    }

    pub fn packet_loss_panel_prev_field(&mut self) {
        if let Some(panel) = &mut self.packet_loss_panel {
            panel.selected_field = panel.selected_field.prev();
        }
    }

    pub fn packet_loss_panel_adjust(&mut self, delta: i32) {
        if let Some(panel) = &mut self.packet_loss_panel {
            if panel.selected_field == PacketLossField::Direction {
                if delta != 0 {
                    panel.direction = match panel.direction {
                        PacketDirection::Outgoing => PacketDirection::Incoming,
                        PacketDirection::Incoming => PacketDirection::Outgoing,
                    };
                }
                return;
            }

            let sim = match panel.direction {
                PacketDirection::Outgoing => &mut panel.sim,
                PacketDirection::Incoming => &mut panel.incoming_sim,
            };

            match panel.selected_field {
                PacketLossField::Direction => unreachable!(),
                PacketLossField::Enabled => {
                    sim.enabled = !sim.enabled;
                }
                PacketLossField::LossPercent => {
                    let new_val = sim.loss_percent + delta as f32;
                    sim.loss_percent = new_val.clamp(0.0, 100.0);
                }
                PacketLossField::MinLatency => {
                    let new_val = sim.min_latency_ms as i32 + delta * 5;
                    sim.min_latency_ms = new_val.clamp(0, 5000) as u32;
                    if sim.min_latency_ms > sim.max_latency_ms {
                        sim.max_latency_ms = sim.min_latency_ms;
                    }
                }
                PacketLossField::MaxLatency => {
                    let new_val = sim.max_latency_ms as i32 + delta * 5;
                    sim.max_latency_ms = new_val.clamp(0, 5000) as u32;
                    if sim.max_latency_ms < sim.min_latency_ms {
                        sim.min_latency_ms = sim.max_latency_ms;
                    }
                }
                PacketLossField::Jitter => {
                    let new_val = sim.jitter_ms as i32 + delta * 5;
                    sim.jitter_ms = new_val.clamp(0, 1000) as u32;
                }
            }
        }
    }

    pub fn take_pending_packet_loss_update(
        &mut self,
    ) -> Option<(u32, PacketLossSimulation, PacketLossSimulation)> {
        self.pending_packet_loss_update.take()
    }

    pub fn packet_loss_panel(&self) -> Option<&PacketLossPanelState> {
        self.packet_loss_panel.as_ref()
    }

    fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}

pub fn render(frame: &mut Frame, state: &TuiState, stats: &ServerStats, clients: &[ClientInfo]) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_header(frame, chunks[0], stats, state.uptime_secs());
    render_tabs(frame, chunks[1], state);

    match state.active_tab {
        Tab::Console => render_console(frame, chunks[2], state),
        Tab::Connections => render_connections(frame, chunks[2], state, clients),
    }

    render_help(frame, chunks[3], state);

    if let Some(panel) = state.packet_loss_panel() {
        render_packet_loss_panel(frame, panel);
    }
}

fn render_header(frame: &mut Frame, area: Rect, stats: &ServerStats, uptime_secs: u64) {
    let uptime = format_duration(uptime_secs);
    let net = &stats.network_stats;

    let text = format!(
        "Tick: {} | Clients: {}/{} | Entities: {} | RTT: {:.0}ms | {} | Uptime: {}",
        stats.tick,
        stats.client_count,
        stats.max_clients,
        stats.entity_count,
        net.rtt_ms,
        format_bytes(net.bytes_sent + net.bytes_received),
        uptime
    );

    let block = Block::default()
        .title(" Dual Server ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(text)
        .block(block)
        .style(Style::default().fg(Color::White));

    frame.render_widget(paragraph, area);
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn render_tabs(frame: &mut Frame, area: Rect, state: &TuiState) {
    let titles: Vec<&str> = Tab::all().iter().map(|t| t.title()).collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL))
        .select(state.active_tab.index())
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

fn render_console(frame: &mut Frame, area: Rect, state: &TuiState) {
    let block = Block::default()
        .title(" Console ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let inner_height = area.height.saturating_sub(2) as usize;
    let total_logs = state.logs.len();
    let start_idx = total_logs.saturating_sub(inner_height + state.scroll_offset);
    let end_idx = total_logs.saturating_sub(state.scroll_offset);

    let lines: Vec<Line> = state
        .logs
        .iter()
        .skip(start_idx)
        .take(end_idx - start_idx)
        .map(|entry| {
            let elapsed = entry.timestamp.elapsed().as_secs();
            let time_str = format!("[{:02}:{:02}]", elapsed / 60, elapsed % 60);

            Line::from(vec![
                Span::styled(time_str, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(
                    entry.level.prefix(),
                    Style::default().fg(entry.level.color()),
                ),
                Span::raw(" "),
                Span::styled(&entry.message, Style::default().fg(Color::White)),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn render_connections(frame: &mut Frame, area: Rect, state: &TuiState, clients: &[ClientInfo]) {
    let block = Block::default()
        .title(format!(" Connections ({}) ", clients.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    if clients.is_empty() {
        let paragraph = Paragraph::new("No clients connected").block(block).style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        );
        frame.render_widget(paragraph, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("ID"),
        Cell::from("Address"),
        Cell::from("Entity"),
        Cell::from("Time"),
        Cell::from("RTT"),
        Cell::from("Sim"),
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(1);

    let rows: Vec<Row> = clients
        .iter()
        .enumerate()
        .map(|(i, client)| {
            let connected = format_duration(client.connected_secs);
            let entity_str = client
                .entity_id
                .map(|e| format!("{}", e))
                .unwrap_or_else(|| "-".to_string());

            let sim_status =
                if client.packet_loss_sim.enabled && client.incoming_packet_loss_sim.enabled {
                    Span::styled("BOTH", Style::default().fg(Color::Red))
                } else if client.packet_loss_sim.enabled {
                    Span::styled("OUT", Style::default().fg(Color::Yellow))
                } else if client.incoming_packet_loss_sim.enabled {
                    Span::styled("IN", Style::default().fg(Color::Yellow))
                } else {
                    Span::raw("-")
                };

            let cells = vec![
                Cell::from(format!("{}", client.client_id)),
                Cell::from(client.addr.as_str()),
                Cell::from(entity_str),
                Cell::from(connected),
                Cell::from(format!("{:.0}ms", client.last_ping_ms)),
                Cell::from(sim_status),
            ];

            let row = Row::new(cells);
            if i == state.selected_connection {
                row.style(Style::default().fg(Color::Black).bg(Color::White))
            } else {
                row.style(Style::default().fg(Color::White))
            }
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(25),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(block);

    frame.render_widget(table, area);
}

fn render_help(frame: &mut Frame, area: Rect, state: &TuiState) {
    let help_text = if state.is_packet_loss_panel_open() {
        "Up/Down: Select | Left/Right: Adjust | Enter: Save | Esc: Cancel"
    } else {
        match state.active_tab {
            Tab::Console => "Tab: Switch | PgUp/PgDn: Scroll | End: Latest | q/Esc: Quit",
            Tab::Connections => {
                "Tab: Switch | Up/Down: Select | Enter: Settings | K: Kick | q/Esc: Quit"
            }
        }
    };

    let block = Block::default()
        .title(" Controls ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(paragraph, area);
}

fn render_packet_loss_panel(frame: &mut Frame, panel: &PacketLossPanelState) {
    let area = frame.area();
    let panel_width = 50;
    let panel_height = 14;
    let x = (area.width.saturating_sub(panel_width)) / 2;
    let y = (area.height.saturating_sub(panel_height)) / 2;
    let panel_area = Rect::new(
        x,
        y,
        panel_width.min(area.width),
        panel_height.min(area.height),
    );

    frame.render_widget(Clear, panel_area);

    let block = Block::default()
        .title(format!(
            " Packet Loss Simulation - Client {} ",
            panel.client_id
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));
    frame.render_widget(block, panel_area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(panel_area);

    let sim = match panel.direction {
        PacketDirection::Outgoing => &panel.sim,
        PacketDirection::Incoming => &panel.incoming_sim,
    };

    let direction_str = match panel.direction {
        PacketDirection::Outgoing => "Outgoing (S->C)",
        PacketDirection::Incoming => "Incoming (C->S)",
    };

    let fields = [
        (
            PacketLossField::Direction,
            "Direction",
            direction_str.to_string(),
        ),
        (
            PacketLossField::Enabled,
            "Enabled",
            if sim.enabled { "Yes" } else { "No" }.to_string(),
        ),
        (
            PacketLossField::LossPercent,
            "Packet Loss",
            format!("{:.1}%", sim.loss_percent),
        ),
        (
            PacketLossField::MinLatency,
            "Min Latency",
            format!("{} ms", sim.min_latency_ms),
        ),
        (
            PacketLossField::MaxLatency,
            "Max Latency",
            format!("{} ms", sim.max_latency_ms),
        ),
        (
            PacketLossField::Jitter,
            "Jitter",
            format!("{} ms", sim.jitter_ms),
        ),
    ];

    for (i, (field, label, value)) in fields.iter().enumerate() {
        let is_selected = panel.selected_field == *field;
        let style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default().fg(Color::White)
        };

        let line = format!("{:<15} {}", format!("{}:", label), value);
        let paragraph = Paragraph::new(line).style(style);
        frame.render_widget(paragraph, inner[i]);
    }
}

fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;
    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{:02}:{:02}", mins, secs)
    }
}
