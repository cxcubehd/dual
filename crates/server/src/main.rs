mod config;
mod events;
mod server;
mod tui;

use std::io;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use config::ServerConfig;
use dual::PacketLossSimulation;
use events::ServerEvent;
use server::GameServer;
use tui::TuiState;

#[derive(Parser)]
#[command(name = "dual-server")]
#[command(about = "Dual game server")]
struct Args {
    #[arg(short, long, default_value = "0.0.0.0")]
    bind: String,

    #[arg(short, long, default_value_t = dual::DEFAULT_PORT)]
    port: u16,

    #[arg(short, long, default_value_t = 60)]
    tick_rate: u32,

    #[arg(short, long, default_value_t = 32)]
    max_clients: usize,

    #[arg(long)]
    headless: bool,

    #[arg(long, help = "Enable global packet loss simulation")]
    simulate_packet_loss: bool,

    #[arg(long, default_value_t = 0.0, help = "Packet loss percentage (0-100)")]
    loss_percent: f32,

    #[arg(long, default_value_t = 0, help = "Minimum latency in ms")]
    min_latency: u32,

    #[arg(long, default_value_t = 0, help = "Maximum latency in ms")]
    max_latency: u32,

    #[arg(long, default_value_t = 0, help = "Jitter in ms")]
    jitter: u32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let bind_addr = format!("{}:{}", args.bind, args.port);

    let global_packet_loss = if args.simulate_packet_loss {
        Some(PacketLossSimulation {
            enabled: true,
            loss_percent: args.loss_percent,
            min_latency_ms: args.min_latency,
            max_latency_ms: args.max_latency,
            jitter_ms: args.jitter,
        })
    } else {
        None
    };

    let config = ServerConfig {
        tick_rate: args.tick_rate,
        max_clients: args.max_clients,
        global_packet_loss,
        ..Default::default()
    };

    let mut server = GameServer::new(&bind_addr, config)?;

    if args.headless {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
        log::info!("Server started on {}", server.local_addr());
        server.run();
        log::info!("Server shutting down");
    } else {
        run_with_tui(&mut server)?;
    }

    Ok(())
}

fn run_with_tui(server: &mut GameServer) -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let running = server.running();
    let mut tui_state = TuiState::new();

    tui_state.log_info(format!("Server started on {}", server.local_addr()));

    while running.load(Ordering::SeqCst) {
        server.tick_once();

        for event in server.drain_events() {
            match event {
                ServerEvent::ClientConnecting { addr } => {
                    tui_state.log_info(format!("Connection request from {}", addr));
                }
                ServerEvent::ClientConnected {
                    client_id,
                    addr,
                    entity_id,
                } => {
                    tui_state.log_info(format!(
                        "Client {} connected from {} (entity {})",
                        client_id, addr, entity_id
                    ));
                }
                ServerEvent::ClientDisconnected { client_id, reason } => {
                    tui_state.log_info(format!("Client {} {}", client_id, reason.as_str()));
                }
                ServerEvent::ConnectionDenied { addr, reason } => {
                    tui_state.log_warn(format!("Connection denied to {}: {}", addr, reason));
                }
                ServerEvent::Error { message } => {
                    tui_state.log_error(message);
                }
            }
        }

        if let Some(client_id) = tui_state.take_pending_kick() {
            server.kick_client(client_id);
        }

        if let Some((client_id, sim, incoming_sim)) = tui_state.take_pending_packet_loss_update() {
            server.set_packet_loss_sim(client_id, sim, incoming_sim);
        }

        if event::poll(Duration::from_millis(1))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let clients = server.client_infos();

                    if tui_state.is_packet_loss_panel_open() {
                        match key.code {
                            KeyCode::Esc => tui_state.cancel_packet_loss_panel(),
                            KeyCode::Enter => tui_state.close_packet_loss_panel(),
                            KeyCode::Up => tui_state.packet_loss_panel_prev_field(),
                            KeyCode::Down => tui_state.packet_loss_panel_next_field(),
                            KeyCode::Left => tui_state.packet_loss_panel_adjust(-1),
                            KeyCode::Right => tui_state.packet_loss_panel_adjust(1),
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => {
                                running.store(false, Ordering::SeqCst);
                            }
                            KeyCode::Tab => tui_state.next_tab(),
                            KeyCode::BackTab => tui_state.prev_tab(),
                            KeyCode::PageUp => tui_state.scroll_up(),
                            KeyCode::PageDown => tui_state.scroll_down(),
                            KeyCode::End => tui_state.scroll_to_bottom(),
                            KeyCode::Up => {
                                if tui_state.active_tab() == tui::Tab::Connections {
                                    tui_state.select_prev_connection(clients.len());
                                }
                            }
                            KeyCode::Down => {
                                if tui_state.active_tab() == tui::Tab::Connections {
                                    tui_state.select_next_connection(clients.len());
                                }
                            }
                            KeyCode::Enter => {
                                if tui_state.active_tab() == tui::Tab::Connections {
                                    tui_state.open_packet_loss_panel(&clients);
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Char('K') => {
                                if tui_state.active_tab() == tui::Tab::Connections {
                                    tui_state.request_kick(&clients);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let stats = server.stats();
        let clients = server.client_infos();
        terminal.draw(|frame| {
            tui::render(frame, &tui_state, &stats, &clients);
        })?;
    }

    tui_state.log_info("Shutting down...");
    server.shutdown_connections();

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, cursor::Show)?;

    Ok(())
}
