mod app;
mod debug;
mod game;
pub mod net;
mod render;

use winit::event_loop::EventLoop;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let event_loop = EventLoop::new()?;
    let mut app = app::App::new();

    event_loop.run_app(&mut app)?;

    Ok(())
}
