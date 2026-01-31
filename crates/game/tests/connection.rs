use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use dual::{
    ClientConnection, ConnectionManager, ConnectionState, NetworkEndpoint, Packet, PacketHeader,
    PacketLossSimulation, PacketType, Reliability,
};

static PORT_COUNTER: AtomicU16 = AtomicU16::new(40000);

fn next_port() -> u16 {
    PORT_COUNTER.fetch_add(10, Ordering::SeqCst)
}

fn generate_salt() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64,
    );
    hasher.finish()
}

fn wait_for_packet(
    endpoint: &mut NetworkEndpoint,
    timeout_ms: u64,
) -> Option<Vec<(Packet, SocketAddr)>> {
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(timeout_ms) {
        let received = endpoint.receive().unwrap();
        if !received.is_empty() {
            return Some(received);
        }
        thread::sleep(Duration::from_millis(1));
    }
    None
}

#[test]
fn test_connection_handshake_full_flow() {
    let port = next_port();
    let server_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let client_addr: SocketAddr = format!("127.0.0.1:{}", port + 1).parse().unwrap();

    let mut server_endpoint = NetworkEndpoint::bind(server_addr).unwrap();
    let mut client_endpoint = NetworkEndpoint::bind(client_addr).unwrap();

    let mut connections = ConnectionManager::new(32);
    let client_salt = generate_salt();

    // Client side connection tracker
    let mut client_conn = ClientConnection::new(server_addr, 0, client_salt);

    client_endpoint.set_remote(server_addr);
    let request = client_conn.send_packet(
        PacketType::ConnectionRequest { client_salt },
        Reliability::Unreliable,
    );
    client_endpoint.send(&request).unwrap();

    let received = wait_for_packet(&mut server_endpoint, 200).expect("No packet received");
    assert_eq!(received.len(), 1);

    let (packet, from_addr) = &received[0];
    match &packet.payload {
        PacketType::ConnectionRequest { client_salt: salt } => {
            assert_eq!(*salt, client_salt);

            let client = connections
                .get_or_create_pending(*from_addr, *salt)
                .unwrap();
            let server_salt = client.server_salt;
            let challenge = client.combined_salt();

            let response = client.send_packet(
                PacketType::ConnectionChallenge {
                    server_salt,
                    challenge,
                },
                Reliability::Reliable,
            );
            server_endpoint.send_to(&response, *from_addr).unwrap();
        }
        _ => panic!("Expected ConnectionRequest"),
    }

    let received = wait_for_packet(&mut client_endpoint, 200).expect("No packet received");
    assert_eq!(received.len(), 1);

    let (packet, _) = &received[0];
    match &packet.payload {
        PacketType::ConnectionChallenge {
            server_salt,
            challenge,
        } => {
            client_conn.process_packet(packet.clone()); // Update client state
            let expected = client_salt ^ server_salt;
            assert_eq!(*challenge, expected);

            let response = client_conn.send_packet(
                PacketType::ChallengeResponse {
                    combined_salt: expected,
                },
                Reliability::Reliable,
            );
            client_endpoint.send(&response).unwrap();
        }
        _ => panic!("Expected ConnectionChallenge"),
    }

    let received = wait_for_packet(&mut server_endpoint, 200).expect("No packet received");
    assert_eq!(received.len(), 1);

    let (packet, from_addr) = &received[0];
    match &packet.payload {
        PacketType::ChallengeResponse { combined_salt } => {
            let client = connections.get_by_addr_mut(from_addr).unwrap();
            assert_eq!(*combined_salt, client.combined_salt());

            client.state = ConnectionState::Connected;
            let client_id = client.client_id;

            let accepted = client.send_packet(
                PacketType::ConnectionAccepted {
                    client_id,
                    entity_id: 1,
                },
                Reliability::Reliable,
            );
            server_endpoint.send_to(&accepted, *from_addr).unwrap();
        }
        _ => panic!("Expected ChallengeResponse"),
    }

    let received = wait_for_packet(&mut client_endpoint, 200).expect("No packet received");
    assert_eq!(received.len(), 1);

    let (packet, _) = &received[0];
    match &packet.payload {
        PacketType::ConnectionAccepted { client_id, .. } => {
            assert!(*client_id > 0);
        }
        _ => panic!("Expected ConnectionAccepted"),
    }

    assert_eq!(connections.connected_count(), 1);
}

#[test]
fn test_connection_denied_server_full() {
    let port = next_port();
    let server_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let client_addr: SocketAddr = format!("127.0.0.1:{}", port + 1).parse().unwrap();

    let mut server_endpoint = NetworkEndpoint::bind(server_addr).unwrap();
    let mut client_endpoint = NetworkEndpoint::bind(client_addr).unwrap();

    let mut connections = ConnectionManager::new(0);
    let client_salt = generate_salt();
    let mut client_conn = ClientConnection::new(server_addr, 0, client_salt);

    client_endpoint.set_remote(server_addr);
    let request = client_conn.send_packet(
        PacketType::ConnectionRequest { client_salt },
        Reliability::Unreliable,
    );
    client_endpoint.send(&request).unwrap();

    let received =
        wait_for_packet(&mut server_endpoint, 200).expect("No packet received on server");
    assert_eq!(received.len(), 1);

    let (packet, from_addr) = &received[0];
    match &packet.payload {
        PacketType::ConnectionRequest { client_salt: salt } => {
            match connections.get_or_create_pending(*from_addr, *salt) {
                Ok(_) => panic!("Should have been denied"),
                Err(reason) => {
                    let header = PacketHeader::new(0, 0, 0, PacketHeader::CHANNEL_UNRELIABLE, 0);
                    let denied = Packet::new(
                        header,
                        PacketType::ConnectionDenied {
                            reason: reason.to_string(),
                        },
                    );
                    server_endpoint.send_to(&denied, *from_addr).unwrap();
                }
            }
        }
        _ => panic!("Expected ConnectionRequest"),
    }

    let received =
        wait_for_packet(&mut client_endpoint, 200).expect("No packet received on client");
    assert_eq!(received.len(), 1);

    let (packet, _) = &received[0];
    match &packet.payload {
        PacketType::ConnectionDenied { reason } => {
            assert!(reason.contains("full"));
        }
        _ => panic!("Expected ConnectionDenied"),
    }
}

#[test]
fn test_invalid_challenge_response_rejected() {
    let port = next_port();
    let server_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let client_addr: SocketAddr = format!("127.0.0.1:{}", port + 1).parse().unwrap();

    let mut server_endpoint = NetworkEndpoint::bind(server_addr).unwrap();
    let mut client_endpoint = NetworkEndpoint::bind(client_addr).unwrap();

    let mut connections = ConnectionManager::new(32);
    let client_salt = generate_salt();
    let mut client_conn = ClientConnection::new(server_addr, 0, client_salt);

    client_endpoint.set_remote(server_addr);
    let request = client_conn.send_packet(
        PacketType::ConnectionRequest { client_salt },
        Reliability::Unreliable,
    );
    client_endpoint.send(&request).unwrap();

    let received = wait_for_packet(&mut server_endpoint, 200).expect("No packet received");
    let (_, from_addr) = &received[0];

    let client = connections
        .get_or_create_pending(*from_addr, client_salt)
        .unwrap();
    let server_salt = client.server_salt;
    let challenge = client.combined_salt();

    let response = client.send_packet(
        PacketType::ConnectionChallenge {
            server_salt,
            challenge,
        },
        Reliability::Reliable,
    );
    server_endpoint.send_to(&response, *from_addr).unwrap();

    let received = wait_for_packet(&mut client_endpoint, 200).expect("No packet received");
    client_conn.process_packet(received[0].0.clone());

    let wrong_salt = 0xDEADBEEF;
    let response = client_conn.send_packet(
        PacketType::ChallengeResponse {
            combined_salt: wrong_salt,
        },
        Reliability::Reliable,
    );
    client_endpoint.send(&response).unwrap();

    let _ = wait_for_packet(&mut server_endpoint, 200).expect("No packet received");

    let client = connections.get_by_addr(from_addr).unwrap();
    assert_eq!(client.state, ConnectionState::Connecting);
    assert_eq!(connections.connected_count(), 0);
}

#[test]
fn test_ping_pong() {
    let port = next_port();
    let server_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let client_addr: SocketAddr = format!("127.0.0.1:{}", port + 1).parse().unwrap();

    let mut server_endpoint = NetworkEndpoint::bind(server_addr).unwrap();
    let mut client_endpoint = NetworkEndpoint::bind(client_addr).unwrap();

    // Minimal connection for tests
    let mut client_conn = ClientConnection::new(server_addr, 0, 0);

    let timestamp = 12345u64;

    client_endpoint.set_remote(server_addr);
    let ping = client_conn.send_packet(PacketType::Ping { timestamp }, Reliability::Unreliable);
    client_endpoint.send(&ping).unwrap();

    let received = wait_for_packet(&mut server_endpoint, 200).expect("No packet received");
    assert_eq!(received.len(), 1);

    let (packet, from_addr) = &received[0];
    match &packet.payload {
        PacketType::Ping { timestamp: ts } => {
            let header = PacketHeader::new(0, 0, 0, PacketHeader::CHANNEL_UNRELIABLE, 0);
            let pong = Packet::new(header, PacketType::Pong { timestamp: *ts });
            server_endpoint.send_to(&pong, *from_addr).unwrap();
        }
        _ => panic!("Expected Ping"),
    }

    let received = wait_for_packet(&mut client_endpoint, 200).expect("No packet received");
    assert_eq!(received.len(), 1);

    let (packet, _) = &received[0];
    match &packet.payload {
        PacketType::Pong { timestamp: ts } => {
            assert_eq!(*ts, timestamp);
        }
        _ => panic!("Expected Pong"),
    }
}

#[test]
fn test_client_command_transmission() {
    use dual::ClientCommand;

    let port = next_port();
    let server_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let client_addr: SocketAddr = format!("127.0.0.1:{}", port + 1).parse().unwrap();

    let mut server_endpoint = NetworkEndpoint::bind(server_addr).unwrap();
    let mut client_endpoint = NetworkEndpoint::bind(client_addr).unwrap();

    let mut client_conn = ClientConnection::new(server_addr, 0, 0);

    let mut command = ClientCommand::new(100, 1);
    command.encode_move_direction([1.0, 0.0, 0.5]);
    command.encode_view_angles(1.5, -0.5);
    command.set_flag(ClientCommand::FLAG_SPRINT, true);
    command.set_flag(ClientCommand::FLAG_JUMP, true);

    client_endpoint.set_remote(server_addr);
    let packet = client_conn.send_packet(
        PacketType::ClientCommand(command.clone()),
        Reliability::Unreliable,
    );
    client_endpoint.send(&packet).unwrap();

    let received = wait_for_packet(&mut server_endpoint, 200).expect("No packet received");
    assert_eq!(received.len(), 1);

    let (packet, _) = &received[0];
    match &packet.payload {
        PacketType::ClientCommand(cmd) => {
            assert_eq!(cmd.tick, 100);
            assert_eq!(cmd.command_sequence, 1);
            assert!(cmd.has_flag(ClientCommand::FLAG_SPRINT));
            assert!(cmd.has_flag(ClientCommand::FLAG_JUMP));
            assert!(!cmd.has_flag(ClientCommand::FLAG_CROUCH));

            let move_dir = cmd.decode_move_direction();
            assert!((move_dir[0] - 1.0).abs() < 0.01);
            assert!((move_dir[2] - 0.5).abs() < 0.01);

            let (yaw, pitch) = cmd.decode_view_angles();
            assert!((yaw - 1.5).abs() < 0.001);
            assert!((pitch - -0.5).abs() < 0.001);
        }
        _ => panic!("Expected ClientCommand"),
    }
}

#[test]
fn test_world_snapshot_transmission() {
    use dual::{EntityState, WorldSnapshot};

    let port = next_port();
    let server_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let client_addr: SocketAddr = format!("127.0.0.1:{}", port + 1).parse().unwrap();

    let mut server_endpoint = NetworkEndpoint::bind(server_addr).unwrap();
    let mut client_endpoint = NetworkEndpoint::bind(client_addr).unwrap();

    let mut snapshot = WorldSnapshot::new(42, 123456789);
    snapshot.last_command_ack = 10;

    let mut entity = EntityState::new(1, 0);
    entity.position = [10.0, 20.0, 30.0];
    entity.encode_velocity([5.0, -2.5, 0.0]);
    snapshot.entities.push(entity);

    let header = PacketHeader::new(0, 0, 0, PacketHeader::CHANNEL_UNRELIABLE, 0);
    let packet = Packet::new(header, PacketType::WorldSnapshot(snapshot));
    server_endpoint.send_to(&packet, client_addr).unwrap();

    let received = wait_for_packet(&mut client_endpoint, 200).expect("No packet received");
    assert_eq!(received.len(), 1);

    let (packet, _) = &received[0];
    match &packet.payload {
        PacketType::WorldSnapshot(snap) => {
            assert_eq!(snap.tick, 42);
            assert_eq!(snap.server_time_ms, 123456789);
            assert_eq!(snap.last_command_ack, 10);
            assert_eq!(snap.entities.len(), 1);

            let entity = &snap.entities[0];
            assert_eq!(entity.entity_id, 1);
            assert!((entity.position[0] - 10.0).abs() < 0.001);
        }
        _ => panic!("Expected WorldSnapshot"),
    }
}

#[test]
fn test_disconnect_packet() {
    let port = next_port();
    let server_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let client_addr: SocketAddr = format!("127.0.0.1:{}", port + 1).parse().unwrap();

    let mut server_endpoint = NetworkEndpoint::bind(server_addr).unwrap();
    let mut client_endpoint = NetworkEndpoint::bind(client_addr).unwrap();
    let mut client_conn = ClientConnection::new(server_addr, 0, 0);

    client_endpoint.set_remote(server_addr);
    let packet = client_conn.send_packet(PacketType::Disconnect, Reliability::Reliable);
    client_endpoint.send(&packet).unwrap();

    let received = wait_for_packet(&mut server_endpoint, 200).expect("No packet received");
    assert_eq!(received.len(), 1);

    let (packet, _) = &received[0];
    assert!(matches!(&packet.payload, PacketType::Disconnect));
}

#[test]
fn test_packet_sequence_numbers() {
    let port = next_port();
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let mut conn = ClientConnection::new(addr, 0, 0);

    let p1 = conn.send_packet(PacketType::Ping { timestamp: 1 }, Reliability::Unreliable);
    let p2 = conn.send_packet(PacketType::Ping { timestamp: 2 }, Reliability::Unreliable);
    let p3 = conn.send_packet(PacketType::Ping { timestamp: 3 }, Reliability::Unreliable);

    assert_eq!(p1.header.sequence, 0);
    assert_eq!(p2.header.sequence, 1);
    assert_eq!(p3.header.sequence, 2);
}

#[test]
fn test_multiple_clients_connect() {
    let port = next_port();
    let server_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();

    let mut server_endpoint = NetworkEndpoint::bind(server_addr).unwrap();
    let mut connections = ConnectionManager::new(32);

    for i in 0..3u16 {
        let client_port = port + 2 + i;
        let client_addr: SocketAddr = format!("127.0.0.1:{}", client_port).parse().unwrap();
        let mut client_endpoint = NetworkEndpoint::bind(client_addr).unwrap();

        let client_salt = generate_salt();
        let mut client_conn = ClientConnection::new(server_addr, 0, client_salt);
        client_endpoint.set_remote(server_addr);

        let request = client_conn.send_packet(
            PacketType::ConnectionRequest { client_salt },
            Reliability::Unreliable,
        );
        client_endpoint.send(&request).unwrap();

        let received = wait_for_packet(&mut server_endpoint, 200).expect("No packet received");
        assert_eq!(received.len(), 1);

        let (packet, from_addr) = &received[0];
        if let PacketType::ConnectionRequest { client_salt: salt } = &packet.payload {
            let client = connections
                .get_or_create_pending(*from_addr, *salt)
                .unwrap();
            client.state = ConnectionState::Connected;
        }
    }

    assert_eq!(connections.connected_count(), 3);
    assert_eq!(connections.total_count(), 3);
}

#[test]
fn test_receive_tracker_zero_sequence() {
    let port = next_port();
    let server_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let client_addr: SocketAddr = format!("127.0.0.1:{}", port + 1).parse().unwrap();

    let mut server_endpoint = NetworkEndpoint::bind(server_addr).unwrap();
    let mut client_endpoint = NetworkEndpoint::bind(client_addr).unwrap();
    let mut client_conn = ClientConnection::new(server_addr, 0, 0);

    client_endpoint.set_remote(server_addr);
    let packet =
        client_conn.send_packet(PacketType::Ping { timestamp: 0 }, Reliability::Unreliable);
    client_endpoint.send(&packet).unwrap();

    let received = wait_for_packet(&mut server_endpoint, 200).expect("No packet received");
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].0.header.sequence, 0);
}

#[test]
fn test_connection_survives_packet_loss() {
    let port = next_port();
    let server_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let client_addr: SocketAddr = format!("127.0.0.1:{}", port + 1).parse().unwrap();

    let mut server_endpoint = NetworkEndpoint::bind(server_addr).unwrap();
    let mut client_endpoint = NetworkEndpoint::bind(client_addr).unwrap();

    let mut connections = ConnectionManager::new(32);
    let client_salt = generate_salt();
    let mut client_conn = ClientConnection::new(server_addr, 0, client_salt);

    client_endpoint.set_remote(server_addr);
    let request = client_conn.send_packet(
        PacketType::ConnectionRequest { client_salt },
        Reliability::Unreliable,
    );
    client_endpoint.send(&request).unwrap();

    let received = wait_for_packet(&mut server_endpoint, 200).expect("No packet received");
    let (packet, from_addr) = &received[0];

    if let PacketType::ConnectionRequest { client_salt: salt } = &packet.payload {
        let client = connections
            .get_or_create_pending(*from_addr, *salt)
            .unwrap();
        let server_salt = client.server_salt;
        let challenge = client.combined_salt();

        let response = client.send_packet(
            PacketType::ConnectionChallenge {
                server_salt,
                challenge,
            },
            Reliability::Reliable,
        );
        server_endpoint.send_to(&response, *from_addr).unwrap();
    }

    let received = wait_for_packet(&mut client_endpoint, 200).expect("No packet received");
    client_conn.process_packet(received[0].0.clone());

    let expected = client_salt ^ connections.iter().next().unwrap().server_salt;
    let response = client_conn.send_packet(
        PacketType::ChallengeResponse {
            combined_salt: expected,
        },
        Reliability::Reliable,
    );
    client_endpoint.send(&response).unwrap();

    let received = wait_for_packet(&mut server_endpoint, 200).expect("No packet received");
    let (_, from_addr) = &received[0];

    let client = connections.get_by_addr_mut(from_addr).unwrap();
    client.state = ConnectionState::Connected;

    client.packet_loss_sim = PacketLossSimulation {
        enabled: true,
        loss_percent: 30.0,
        min_latency_ms: 30,
        max_latency_ms: 60,
        jitter_ms: 20,
    };

    let client_id = client.client_id;
    let accepted = client.send_packet(
        PacketType::ConnectionAccepted {
            client_id,
            entity_id: 1,
        },
        Reliability::Reliable,
    );
    server_endpoint.send_to(&accepted, *from_addr).unwrap();

    let _ = wait_for_packet(&mut client_endpoint, 200);

    let test_duration = Duration::from_secs(5); // Reduced duration for unit test
    let start = Instant::now();
    let mut last_send = Instant::now();
    let send_interval = Duration::from_millis(16);

    while start.elapsed() < test_duration {
        if last_send.elapsed() >= send_interval {
            let client = connections.iter_mut().next().unwrap();

            if !client.packet_loss_sim.should_drop() {
                let snapshot = dual::WorldSnapshot::new(
                    client.send_sequence,
                    start.elapsed().as_millis() as u64,
                );
                let packet = client
                    .send_packet(PacketType::WorldSnapshot(snapshot), Reliability::Unreliable);
                let _ = server_endpoint.send_to(&packet, client.addr);
            }

            // Also send resends
            for packet in client.collect_resends() {
                let _ = server_endpoint.send_to(&packet, client.addr);
            }

            last_send = Instant::now();
        }

        if let Ok(received) = client_endpoint.receive() {
            for (packet, _) in received {
                client_conn.process_packet(packet);
            }
        }

        // Client Resends
        for packet in client_conn.collect_resends() {
            let _ = client_endpoint.send(&packet);
        }

        if let Ok(received) = server_endpoint.receive() {
            for (packet, addr) in received {
                if let Some(client) = connections.get_by_addr_mut(&addr) {
                    client.process_packet(packet); // Needed to process ACKs!
                }
            }
        }

        let ping = client_conn.send_packet(
            PacketType::Ping {
                timestamp: start.elapsed().as_millis() as u64,
            },
            Reliability::Unreliable,
        );
        let _ = client_endpoint.send(&ping);

        let timed_out_clients = connections.cleanup_timed_out();
        assert!(
            timed_out_clients.is_empty(),
            "Client was timed out after {:?} - this should not happen with proper timeout handling",
            start.elapsed()
        );

        thread::sleep(Duration::from_millis(16));
    }

    assert_eq!(
        connections.connected_count(),
        1,
        "Connection should survive with 30% packet loss"
    );
}
