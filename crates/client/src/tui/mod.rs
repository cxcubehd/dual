mod screens;

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::net::{ClientConfig, NetworkClient};

pub use screens::Screen;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    LaunchGame,
    Connect(SocketAddr),
    Disconnect,
    ChangeScreen(Screen),
}

pub struct Tui {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    screen: Screen,
    client: Option<NetworkClient>,
    connect_input: String,
    connect_error: Option<String>,
    selected_index: usize,
    should_quit: bool,
    should_launch: bool,
}

impl Tui {
    pub fn new() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, cursor::Hide)?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            screen: Screen::MainMenu,
            client: None,
            connect_input: String::from("127.0.0.1:27015"),
            connect_error: None,
            selected_index: 0,
            should_quit: false,
            should_launch: false,
        })
    }

    pub fn run(&mut self) -> io::Result<Option<NetworkClient>> {
        while !self.should_quit && !self.should_launch {
            self.draw()?;

            if let Some(client) = &mut self.client {
                let _ = client.update(0.016, None);

                if client.is_connected() && self.screen == Screen::Connecting {
                    self.should_launch = true;
                    continue;
                }
            }

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        let action = self.handle_key(key.code, key.modifiers);
                        self.process_action(action)?;
                    }
                }
            }
        }

        Ok(if self.should_launch {
            self.client.take()
        } else {
            None
        })
    }

    fn draw(&mut self) -> io::Result<()> {
        let screen = self.screen;
        let selected = self.selected_index;
        let connect_input = self.connect_input.clone();
        let connect_error = self.connect_error.clone();
        let client = &self.client;

        self.terminal.draw(|frame| {
            screens::render(
                frame,
                screen,
                selected,
                &connect_input,
                connect_error.as_deref(),
                client,
            );
        })?;

        Ok(())
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Action {
        if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            return Action::Quit;
        }

        match self.screen {
            Screen::MainMenu => self.handle_main_menu_key(code),
            Screen::Connect => self.handle_connect_key(code),
            Screen::Connecting => self.handle_connecting_key(code),
            Screen::Connected => self.handle_connected_key(code),
            Screen::InGame => self.handle_in_game_key(code),
        }
    }

    fn handle_main_menu_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_index = self.selected_index.saturating_sub(1);
                Action::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_index = (self.selected_index + 1).min(2);
                Action::None
            }
            KeyCode::Enter => match self.selected_index {
                0 => Action::ChangeScreen(Screen::Connect),
                1 => Action::ChangeScreen(Screen::Connect),
                2 => Action::Quit,
                _ => Action::None,
            },
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            _ => Action::None,
        }
    }

    fn handle_connect_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Esc => {
                self.connect_error = None;
                Action::ChangeScreen(Screen::MainMenu)
            }
            KeyCode::Enter => {
                if let Ok(addr) = self.connect_input.parse() {
                    self.connect_error = None;
                    Action::Connect(addr)
                } else {
                    self.connect_error = Some("Invalid address format".to_string());
                    Action::None
                }
            }
            KeyCode::Backspace => {
                self.connect_input.pop();
                Action::None
            }
            KeyCode::Char(c) => {
                if c.is_ascii_digit() || c == '.' || c == ':' {
                    self.connect_input.push(c);
                }
                Action::None
            }
            _ => Action::None,
        }
    }

    fn handle_connecting_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Esc => {
                if let Some(client) = &mut self.client {
                    let _ = client.disconnect();
                }
                self.client = None;
                Action::ChangeScreen(Screen::MainMenu)
            }
            _ => Action::None,
        }
    }

    #[allow(dead_code)]
    fn handle_connected_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Enter => Action::LaunchGame,
            KeyCode::Esc | KeyCode::Char('q') => Action::Disconnect,
            _ => Action::None,
        }
    }

    #[allow(dead_code)]
    fn handle_in_game_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Esc => Action::ChangeScreen(Screen::Connected),
            KeyCode::Char('q') => Action::Disconnect,
            _ => Action::None,
        }
    }

    fn process_action(&mut self, action: Action) -> io::Result<()> {
        match action {
            Action::None => {}
            Action::Quit => {
                self.should_quit = true;
            }
            Action::LaunchGame => {
                self.should_launch = true;
            }
            Action::Connect(addr) => {
                self.connect_to_server(addr)?;
            }
            Action::Disconnect => {
                if let Some(client) = &mut self.client {
                    let _ = client.disconnect();
                }
                self.client = None;
                self.screen = Screen::MainMenu;
                self.selected_index = 0;
            }
            Action::ChangeScreen(screen) => {
                self.screen = screen;
                self.selected_index = 0;
            }
        }

        Ok(())
    }

    fn connect_to_server(&mut self, addr: SocketAddr) -> io::Result<()> {
        let config = ClientConfig::default();
        let mut client = NetworkClient::new(config)?;

        if let Err(e) = client.connect(addr) {
            self.connect_error = Some(format!("Connection failed: {}", e));
            return Ok(());
        }

        self.client = Some(client);
        self.screen = Screen::Connecting;

        Ok(())
    }

    pub fn restore_terminal(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            cursor::Show
        )?;
        Ok(())
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = self.restore_terminal();
    }
}

pub fn run_menu() -> io::Result<Option<NetworkClient>> {
    let mut tui = Tui::new()?;
    let result = tui.run();
    tui.restore_terminal()?;
    result
}
