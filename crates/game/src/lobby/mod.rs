use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use serde::{Deserialize, Serialize};

pub type PlayerId = u32;
pub type LobbyId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LobbyState {
    Waiting,
    Countdown,
    InGame,
    Finished,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbySettings {
    pub name: String,
    pub max_players: u8,
    pub password: Option<String>,
    pub map_name: String,
    pub game_mode: String,
    pub countdown_secs: u8,
    pub public: bool,
}

impl Default for LobbySettings {
    fn default() -> Self {
        Self {
            name: String::from("Game Lobby"),
            max_players: 16,
            password: None,
            map_name: String::from("default"),
            game_mode: String::from("deathmatch"),
            countdown_secs: 10,
            public: true,
        }
    }
}

#[derive(Debug)]
pub struct Lobby {
    pub id: LobbyId,
    pub settings: LobbySettings,
    pub state: LobbyState,
    pub players: Vec<PlayerId>,
    pub host: PlayerId,
    pub created_at: Instant,
    pub countdown_start: Option<Instant>,
}

impl Lobby {
    pub fn new(id: LobbyId, host: PlayerId, settings: LobbySettings) -> Self {
        Self {
            id,
            settings,
            state: LobbyState::Waiting,
            players: vec![host],
            host,
            created_at: Instant::now(),
            countdown_start: None,
        }
    }

    pub fn player_count(&self) -> u8 {
        self.players.len() as u8
    }

    pub fn is_full(&self) -> bool {
        self.players.len() >= self.settings.max_players as usize
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    pub fn has_password(&self) -> bool {
        self.settings.password.is_some()
    }

    pub fn add_player(&mut self, player_id: PlayerId) -> bool {
        if self.is_full() || self.players.contains(&player_id) {
            return false;
        }
        self.players.push(player_id);
        true
    }

    pub fn remove_player(&mut self, player_id: PlayerId) -> bool {
        if let Some(pos) = self.players.iter().position(|&p| p == player_id) {
            self.players.remove(pos);
            if self.host == player_id && !self.players.is_empty() {
                self.host = self.players[0];
            }
            true
        } else {
            false
        }
    }

    pub fn start_countdown(&mut self) {
        if self.state == LobbyState::Waiting {
            self.state = LobbyState::Countdown;
            self.countdown_start = Some(Instant::now());
        }
    }

    pub fn cancel_countdown(&mut self) {
        if self.state == LobbyState::Countdown {
            self.state = LobbyState::Waiting;
            self.countdown_start = None;
        }
    }

    pub fn countdown_remaining(&self) -> Option<u8> {
        if let (LobbyState::Countdown, Some(start)) = (self.state, self.countdown_start) {
            let elapsed = start.elapsed().as_secs() as u8;
            Some(self.settings.countdown_secs.saturating_sub(elapsed))
        } else {
            None
        }
    }

    pub fn to_info(&self) -> super::net::LobbyInfo {
        super::net::LobbyInfo {
            id: self.id,
            name: self.settings.name.clone(),
            player_count: self.player_count(),
            max_players: self.settings.max_players,
            has_password: self.has_password(),
            map_name: self.settings.map_name.clone(),
            game_mode: self.settings.game_mode.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Queue {
    players: VecDeque<(PlayerId, Instant)>,
    target_lobby_size: u8,
}

impl Queue {
    pub fn new(target_lobby_size: u8) -> Self {
        Self {
            players: VecDeque::new(),
            target_lobby_size,
        }
    }

    pub fn enqueue(&mut self, player_id: PlayerId) -> bool {
        if self.players.iter().any(|(id, _)| *id == player_id) {
            return false;
        }
        self.players.push_back((player_id, Instant::now()));
        true
    }

    pub fn dequeue(&mut self, player_id: PlayerId) -> bool {
        if let Some(pos) = self.players.iter().position(|(id, _)| *id == player_id) {
            self.players.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn position(&self, player_id: PlayerId) -> Option<u32> {
        self.players
            .iter()
            .position(|(id, _)| *id == player_id)
            .map(|p| p as u32 + 1)
    }

    pub fn estimated_wait_secs(&self, player_id: PlayerId) -> Option<u32> {
        self.position(player_id).map(|pos| {
            let matches_needed = pos / self.target_lobby_size as u32;
            matches_needed * 60
        })
    }

    pub fn pop_match(&mut self) -> Option<Vec<PlayerId>> {
        if self.players.len() >= self.target_lobby_size as usize {
            let players: Vec<PlayerId> = self
                .players
                .drain(..self.target_lobby_size as usize)
                .map(|(id, _)| id)
                .collect();
            Some(players)
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.players.len()
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct LobbyManager {
    lobbies: HashMap<LobbyId, Lobby>,
    player_lobbies: HashMap<PlayerId, LobbyId>,
    next_lobby_id: LobbyId,
}

impl LobbyManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_lobby(&mut self, host: PlayerId, settings: LobbySettings) -> LobbyId {
        let id = self.next_lobby_id;
        self.next_lobby_id += 1;

        let lobby = Lobby::new(id, host, settings);
        self.lobbies.insert(id, lobby);
        self.player_lobbies.insert(host, id);

        id
    }

    pub fn join_lobby(
        &mut self,
        lobby_id: LobbyId,
        player_id: PlayerId,
        password: Option<&str>,
    ) -> Result<(), &'static str> {
        if self.player_lobbies.contains_key(&player_id) {
            return Err("Already in a lobby");
        }

        let lobby = self.lobbies.get_mut(&lobby_id).ok_or("Lobby not found")?;

        if lobby.is_full() {
            return Err("Lobby is full");
        }

        if let Some(ref required) = lobby.settings.password {
            match password {
                Some(provided) if provided == required => {}
                _ => return Err("Invalid password"),
            }
        }

        lobby.add_player(player_id);
        self.player_lobbies.insert(player_id, lobby_id);

        Ok(())
    }

    pub fn leave_lobby(&mut self, player_id: PlayerId) -> Option<LobbyId> {
        let lobby_id = self.player_lobbies.remove(&player_id)?;
        let lobby = self.lobbies.get_mut(&lobby_id)?;

        lobby.remove_player(player_id);

        if lobby.is_empty() {
            self.lobbies.remove(&lobby_id);
        }

        Some(lobby_id)
    }

    pub fn get(&self, lobby_id: LobbyId) -> Option<&Lobby> {
        self.lobbies.get(&lobby_id)
    }

    pub fn get_mut(&mut self, lobby_id: LobbyId) -> Option<&mut Lobby> {
        self.lobbies.get_mut(&lobby_id)
    }

    pub fn player_lobby(&self, player_id: PlayerId) -> Option<LobbyId> {
        self.player_lobbies.get(&player_id).copied()
    }

    pub fn list_public(&self) -> Vec<super::net::LobbyInfo> {
        self.lobbies
            .values()
            .filter(|l| l.settings.public && l.state == LobbyState::Waiting)
            .map(|l| l.to_info())
            .collect()
    }

    pub fn lobby_count(&self) -> usize {
        self.lobbies.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lobby_lifecycle() {
        let mut manager = LobbyManager::new();

        let lobby_id = manager.create_lobby(1, LobbySettings::default());
        assert!(manager.join_lobby(lobby_id, 2, None).is_ok());
        assert!(manager.join_lobby(lobby_id, 1, None).is_err());

        let lobby = manager.get(lobby_id).unwrap();
        assert_eq!(lobby.players.len(), 2);

        manager.leave_lobby(1);
        let lobby = manager.get(lobby_id).unwrap();
        assert_eq!(lobby.host, 2);
    }

    #[test]
    fn test_queue() {
        let mut queue = Queue::new(4);

        queue.enqueue(1);
        queue.enqueue(2);
        queue.enqueue(3);

        assert_eq!(queue.position(2), Some(2));
        assert!(queue.pop_match().is_none());

        queue.enqueue(4);
        let match_players = queue.pop_match().unwrap();
        assert_eq!(match_players, vec![1, 2, 3, 4]);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_lobby_password() {
        let mut manager = LobbyManager::new();

        let settings = LobbySettings {
            password: Some("secret".to_string()),
            ..Default::default()
        };

        let lobby_id = manager.create_lobby(1, settings);

        assert!(manager.join_lobby(lobby_id, 2, None).is_err());
        assert!(manager.join_lobby(lobby_id, 2, Some("wrong")).is_err());
        assert!(manager.join_lobby(lobby_id, 2, Some("secret")).is_ok());
    }
}
