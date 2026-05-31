#[cfg(feature = "client")]
mod app;
#[cfg(feature = "client")]
mod assets;
#[cfg(feature = "client")]
mod debug;
#[cfg(feature = "client")]
mod game;
#[cfg(feature = "client")]
mod render;
#[cfg(feature = "server")]
mod server_app;
#[cfg(feature = "client")]
mod tui;

#[cfg(feature = "client")]
use std::net::SocketAddr;

#[cfg(any(feature = "client", feature = "server"))]
use clap::Parser;

#[cfg(feature = "client")]
use dual::{ClientConfig, NetworkClient};

#[cfg(feature = "client")]
#[derive(Parser, Debug)]
#[command(name = "dual")]
#[command(about = "Dual game client")]
struct ClientArgs {
    #[arg(
        short = 'c',
        long = "connect",
        help = "Server address to connect to (e.g., 127.0.0.1:27015)"
    )]
    server_addr: Option<String>,

    #[arg(long, help = "Skip TUI menu and launch game directly")]
    skip_menu: bool,
}

#[cfg(all(feature = "client", feature = "server"))]
#[derive(Parser, Debug)]
#[command(name = "dual")]
#[command(about = "Dual client/server launcher")]
struct Args {
    #[arg(long, conflicts_with = "server", help = "Run the graphical client")]
    client: bool,

    #[arg(long, conflicts_with = "client", help = "Run the dedicated server")]
    server: bool,

    #[command(flatten)]
    client_args: ClientArgs,

    #[command(flatten)]
    server_args: server_app::ServerArgs,
}

#[cfg(all(feature = "client", not(feature = "server")))]
fn main() -> anyhow::Result<()> {
    let args = ClientArgs::parse();
    run_client(args)
}

#[cfg(all(feature = "server", not(feature = "client")))]
fn main() -> anyhow::Result<()> {
    let args = server_app::ServerArgs::parse();
    server_app::run(args)
}

#[cfg(all(feature = "client", feature = "server"))]
fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.client {
        return run_client(args.client_args);
    }

    if args.server {
        return server_app::run(args.server_args);
    }

    anyhow::bail!("select a mode with --client or --server");
}

#[cfg(not(any(feature = "client", feature = "server")))]
fn main() {
    println!(
        "Dual standalone build. Enable the client, server, or both feature to run an interactive mode."
    );
}

#[cfg(feature = "client")]
fn run_client(args: ClientArgs) -> anyhow::Result<()> {
    let _ = env_logger::try_init();

    if let Some(server_addr) = args.server_addr {
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

#[cfg(feature = "client")]
fn connect_to_server(addr: &str) -> anyhow::Result<NetworkClient> {
    let socket_addr: SocketAddr = addr.parse()?;
    let config = ClientConfig::default();
    let mut client = NetworkClient::new(config)?;
    client.connect(socket_addr)?;
    Ok(client)
}

#[cfg(feature = "client")]
fn run_game(client: Option<NetworkClient>) -> anyhow::Result<()> {
    let event_loop = winit::event_loop::EventLoop::new()?;
    let mut app = app::App::with_network_client(client);
    event_loop.run_app(&mut app)?;
    Ok(())
}
