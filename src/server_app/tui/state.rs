use std::collections::VecDeque;
use std::time::Instant;

use dual::{PacketLossSimulation, ServerClientInfo};
use ratatui::style::Color;

use super::packet_loss::PacketLossPanelState;

pub(super) const MAX_LOG_ENTRIES: usize = 1000;
pub(super) const VISIBLE_LOG_LINES: usize = 20;

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
    pub(super) fn color(&self) -> Color {
        match self {
            LogLevel::Info => Color::White,
            LogLevel::Warn => Color::Yellow,
            LogLevel::Error => Color::Red,
        }
    }

    pub(super) fn prefix(&self) -> &'static str {
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
    pub(super) fn all() -> &'static [Tab] {
        &[Tab::Console, Tab::Connections]
    }

    pub(super) fn title(&self) -> &'static str {
        match self {
            Tab::Console => "Console",
            Tab::Connections => "Connections",
        }
    }

    pub(super) fn index(&self) -> usize {
        match self {
            Tab::Console => 0,
            Tab::Connections => 1,
        }
    }
}

pub struct TuiState {
    pub(super) logs: VecDeque<LogEntry>,
    pub(super) scroll_offset: usize,
    pub(super) start_time: Instant,
    pub(super) active_tab: Tab,
    pub(super) selected_connection: usize,
    pending_kick: Option<u32>,
    pub(super) packet_loss_panel: Option<PacketLossPanelState>,
    pub(super) pending_packet_loss_update:
        Option<(u32, PacketLossSimulation, PacketLossSimulation)>,
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

    pub fn request_kick(&mut self, clients: &[ServerClientInfo]) {
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

    pub(super) fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}
