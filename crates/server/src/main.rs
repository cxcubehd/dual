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
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use server::{GameServer, ServerConfig};

#[derive(Parser)]
#[command(name = "dual-server")]
#[command(about = "Dual game server")]
struct Args {
    #[arg(short, long, default_value = "0.0.0.0")]
    bind: String,

    #[arg(short, long, default_value_t = dual_game::DEFAULT_PORT)]
    port: u16,

    #[arg(short, long, default_value_t = 60)]
    tick_rate: u32,

    #[arg(short, long, default_value_t = 32)]
    max_clients: usize,

    #[arg(long)]
    headless: bool,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();
    let bind_addr = format!("{}:{}", args.bind, args.port);

    let config = ServerConfig {
        tick_rate: args.tick_rate,
        max_clients: args.max_clients,
        ..Default::default()
    };

    let mut server = GameServer::new(&bind_addr, config)?;

    if args.headless {
        server.run();
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

    while running.load(Ordering::SeqCst) {
        server.tick_once();

        if event::poll(Duration::from_millis(1))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            running.store(false, Ordering::SeqCst);
                        }
                        _ => {}
                    }
                }
            }
        }

        terminal.draw(|frame| {
            tui::render(frame, server.stats());
        })?;
    }

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, cursor::Show)?;

    Ok(())
}
