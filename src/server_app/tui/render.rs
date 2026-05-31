use dual::{ServerClientInfo, ServerStats};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Tabs};

use super::Tab;
use super::format::{format_bytes, format_duration};
use super::packet_loss::{PacketDirection, PacketLossField, PacketLossPanelState};
use super::state::TuiState;

pub fn render(
    frame: &mut Frame,
    state: &TuiState,
    stats: &ServerStats,
    clients: &[ServerClientInfo],
) {
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

fn render_connections(
    frame: &mut Frame,
    area: Rect,
    state: &TuiState,
    clients: &[ServerClientInfo],
) {
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
