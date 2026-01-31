mod app;
mod assets;
mod debug;
mod game;
pub mod net;
mod render;
mod tui;

use std::net::SocketAddr;

use clap::Parser;
use winit::event_loop::EventLoop;

use net::{ClientConfig, NetworkClient};

#[derive(Parser)]
#[command(name = "dual")]
#[command(about = "Dual game client")]
struct Args {
    #[arg(
        short,
        long,
        help = "Server address to connect to (e.g., 127.0.0.1:27015)"
    )]
    server: Option<String>,

    #[arg(long, help = "Skip TUI menu and launch game directly")]
    skip_menu: bool,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    if let Some(server_addr) = args.server {
        let client = connect_to_server(&server_addr)?;
        run_game(Some(client))?;
        return Ok(());
    }

    if args.skip_menu {
        run_game(None)?;
        return Ok(());
    }

    match tui::run_menu() {
        Ok(Some(client)) => {
            run_game(Some(client))?;
        }
        Ok(None) => {
            log::info!("Exiting from menu");
        }
        Err(e) => {
            eprintln!("TUI error: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

fn connect_to_server(addr: &str) -> anyhow::Result<NetworkClient> {
    let socket_addr: SocketAddr = addr.parse()?;
    let config = ClientConfig::default();
    let mut client = NetworkClient::new(config)?;
    client.connect(socket_addr)?;
    Ok(client)
}

fn run_game(client: Option<NetworkClient>) -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = app::App::with_network_client(client);
    event_loop.run_app(&mut app)?;
    Ok(())
}
