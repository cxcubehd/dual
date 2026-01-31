use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::net::NetworkClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    Connect,
    Connecting,
    #[allow(dead_code)]
    Connected,
    #[allow(dead_code)]
    InGame,
}

pub fn render(
    frame: &mut Frame,
    screen: Screen,
    selected: usize,
    connect_input: &str,
    connect_error: Option<&str>,
    client: &Option<NetworkClient>,
) {
    let area = frame.area();

    let block = Block::default()
        .title(" Dual ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Min(0)])
        .split(area)[0];

    match screen {
        Screen::MainMenu => render_main_menu(frame, inner, selected),
        Screen::Connect => render_connect(frame, inner, connect_input, connect_error),
        Screen::Connecting => render_connecting(frame, inner, client),
        Screen::Connected => render_connected(frame, inner, client),
        Screen::InGame => render_in_game_menu(frame, inner),
    }
}

fn render_main_menu(frame: &mut Frame, area: Rect, selected: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);

    let title = r#"
  ____  _   _   _    _     
 |  _ \| | | | / \  | |    
 | | | | | | |/ _ \ | |    
 | |_| | |_| / ___ \| |___ 
 |____/ \___/_/   \_\_____|
"#;

    let title_widget = Paragraph::new(title)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center);
    frame.render_widget(title_widget, chunks[0]);

    let menu_items = vec![
        ListItem::new("  Connect to Server"),
        ListItem::new("  Server Browser"),
        ListItem::new("  Quit"),
    ];

    let menu_items: Vec<ListItem> = menu_items
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            if i == selected {
                item.style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                item.style(Style::default().fg(Color::White))
            }
        })
        .collect();

    let menu = List::new(menu_items).block(
        Block::default()
            .title(" Menu ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    let menu_area = centered_rect(40, 8, chunks[2]);
    frame.render_widget(menu, menu_area);

    let help = Paragraph::new("↑↓ Navigate  Enter Select  Q Quit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help, chunks[3]);
}

fn render_connect(frame: &mut Frame, area: Rect, input: &str, error: Option<&str>) {
    let _chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(area);

    let dialog_area = centered_rect(50, 10, area);
    frame.render_widget(Clear, dialog_area);

    let dialog = Block::default()
        .title(" Connect to Server ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(dialog, dialog_area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(dialog_area);

    let label = Paragraph::new("Server Address:").style(Style::default().fg(Color::White));
    frame.render_widget(label, inner[0]);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let input_text = Paragraph::new(format!("{}_", input))
        .style(Style::default().fg(Color::White))
        .block(input_block);
    frame.render_widget(input_text, inner[1]);

    if let Some(err) = error {
        let error_text = Paragraph::new(err)
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center);
        frame.render_widget(error_text, inner[2]);
    }

    let help = Paragraph::new("Enter Connect  Esc Cancel")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help, inner[3]);
}

fn render_connecting(frame: &mut Frame, area: Rect, client: &Option<NetworkClient>) {
    let dialog_area = centered_rect(40, 8, area);
    frame.render_widget(Clear, dialog_area);

    let dialog = Block::default()
        .title(" Connecting ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    frame.render_widget(dialog, dialog_area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(dialog_area);

    let status = if let Some(client) = client {
        let state = format!("{:?}", client.state());
        format!("Status: {}\n\nPlease wait...", state)
    } else {
        "Initializing connection...".to_string()
    };

    let status_text = Paragraph::new(status)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center);
    frame.render_widget(status_text, inner[0]);

    let help = Paragraph::new("Esc Cancel")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help, inner[1]);
}

#[allow(dead_code)]
fn render_connected(frame: &mut Frame, area: Rect, client: &Option<NetworkClient>) {
    let dialog_area = centered_rect(50, 12, area);
    frame.render_widget(Clear, dialog_area);

    let dialog = Block::default()
        .title(" Connected ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));
    frame.render_widget(dialog, dialog_area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(dialog_area);

    if let Some(client) = client {
        let client_id = client
            .client_id()
            .map(|id| format!("Client ID: {}", id))
            .unwrap_or_else(|| "Client ID: -".to_string());

        let stats = client.stats();
        let rtt = format!("RTT: {:.1}ms", stats.rtt_ms);
        let packets = format!(
            "Packets: {} sent / {} recv",
            stats.packets_sent, stats.packets_received
        );

        let lines = vec![
            Line::from(Span::styled(
                "Connected to server!",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(client_id, Style::default().fg(Color::White))),
            Line::from(Span::styled(rtt, Style::default().fg(Color::Cyan))),
            Line::from(Span::styled(packets, Style::default().fg(Color::DarkGray))),
        ];

        let info = Paragraph::new(lines).alignment(Alignment::Center);
        frame.render_widget(info, inner[0]);
    }

    let action = Paragraph::new("Press ENTER to launch game")
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);
    frame.render_widget(action, inner[4]);

    let help = Paragraph::new("Enter Launch  Esc/Q Disconnect")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help, inner[5]);
}

fn render_in_game_menu(frame: &mut Frame, area: Rect) {
    let dialog_area = centered_rect(40, 10, area);
    frame.render_widget(Clear, dialog_area);

    let dialog = Block::default()
        .title(" Game Menu ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(dialog, dialog_area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(dialog_area);

    let title = Paragraph::new("Game Paused")
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);
    frame.render_widget(title, inner[0]);

    let options = Paragraph::new("Q - Leave Match\nEsc - Resume Game")
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center);
    frame.render_widget(options, inner[1]);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
