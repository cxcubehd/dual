#![allow(unused)]

mod app;
mod assets;
mod debug;
mod game;
pub mod net;
mod render;
mod tui;

use clap::Parser;
use winit::event_loop::EventLoop;

#[derive(Parser)]
#[command(name = "dual")]
#[command(about = "Dual game client")]
struct Args {
    #[arg(short, long)]
    server: Option<String>,

    #[arg(long)]
    skip_menu: bool,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    if args.skip_menu {
        launch_game(None)?;
    } else {
        match tui::run_menu() {
            Ok(Some(client)) => {
                launch_game(Some(client))?;
            }
            Ok(None) => {
                log::info!("Exiting from menu");
            }
            Err(e) => {
                eprintln!("TUI error: {}", e);
                return Err(e.into());
            }
        }
    }

    Ok(())
}

fn launch_game(client: Option<net::NetworkClient>) -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = app::App::with_network_client(client);

    event_loop.run_app(&mut app)?;

    Ok(())
}
