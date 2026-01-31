use std::collections::VecDeque;
use std::time::Instant;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs};
use ratatui::Frame;

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

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub client_id: u32,
    pub addr: String,
    pub entity_id: Option<u32>,
    pub connected_secs: u64,
    pub last_ping_ms: f32,
}

pub struct TuiState {
    logs: VecDeque<LogEntry>,
    scroll_offset: usize,
    start_time: Instant,
    active_tab: Tab,
    selected_connection: usize,
    pending_kick: Option<u32>,
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

    let items: Vec<ListItem> = clients
        .iter()
        .enumerate()
        .map(|(i, client)| {
            let connected = format_duration(client.connected_secs);
            let entity_str = client
                .entity_id
                .map(|e| format!("E#{}", e))
                .unwrap_or_else(|| "-".to_string());

            let content = format!(
                "#{:02} | {} | {} | {} | RTT: {:.0}ms",
                client.client_id, client.addr, entity_str, connected, client.last_ping_ms
            );

            let style = if i == state.selected_connection {
                Style::default().fg(Color::Black).bg(Color::White)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_help(frame: &mut Frame, area: Rect, state: &TuiState) {
    let help_text = match state.active_tab {
        Tab::Console => "Tab: Switch | PgUp/PgDn: Scroll | End: Latest | q/Esc: Quit",
        Tab::Connections => "Tab: Switch | Up/Down: Select | K: Kick | q/Esc: Quit",
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
