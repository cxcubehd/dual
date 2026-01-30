use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::Frame;

use crate::server::ServerStats;

pub fn render(frame: &mut Frame, stats: ServerStats) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(7),
            Constraint::Min(0),
        ])
        .split(frame.area());

    render_header(frame, chunks[0], &stats);
    render_status(frame, chunks[1], &stats);
    render_network(frame, chunks[2], &stats);
    render_help(frame, chunks[3]);
}

fn render_header(frame: &mut Frame, area: Rect, stats: &ServerStats) {
    let uptime = format_duration(stats.uptime_secs);
    let title = format!(" Dual Server - Uptime: {} ", uptime);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let text = format!(
        "Tick: {}  |  Clients: {}  |  Entities: {}",
        stats.tick, stats.client_count, stats.entity_count
    );

    let paragraph = Paragraph::new(text)
        .block(block)
        .style(Style::default().fg(Color::White));

    frame.render_widget(paragraph, area);
}

fn render_status(frame: &mut Frame, area: Rect, stats: &ServerStats) {
    let block = Block::default()
        .title(" Status ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let client_ratio = stats.client_count as f64 / 32.0;
    let gauge = Gauge::default()
        .block(block)
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(client_ratio.min(1.0))
        .label(format!("{}/32 clients", stats.client_count));

    frame.render_widget(gauge, area);
}

fn render_network(frame: &mut Frame, area: Rect, stats: &ServerStats) {
    let block = Block::default()
        .title(" Network ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let net = &stats.network_stats;
    let lines = vec![
        Line::from(vec![
            Span::styled("Packets: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} sent / {} recv", net.packets_sent, net.packets_received),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Bytes: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!(
                    "{} sent / {} recv",
                    format_bytes(net.bytes_sent),
                    format_bytes(net.bytes_received)
                ),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("RTT: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{:.1}ms (+/- {:.1}ms)", net.rtt_ms, net.rtt_variance),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Packet Loss: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{:.1}%", net.packet_loss_percent),
                Style::default().fg(if net.packet_loss_percent > 5.0 {
                    Color::Red
                } else {
                    Color::White
                }),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn render_help(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Controls ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let text = Paragraph::new("Press 'q' or ESC to quit")
        .block(block)
        .style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        );

    frame.render_widget(text, area);
}

fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
