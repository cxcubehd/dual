#![allow(unused)]

use glam::{Mat4, Quat, Vec3};
use rkyv::{Archive, Deserialize, Portable, Serialize, deserialize, rancor::Error};

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub enum Team {
    Red,
    Blue,
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub struct Player {
    pub id: i16,
    pub name: String,
    pub health: i32,
    pub position: Vec3,
    pub rotation: Quat,
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub enum Entity {
    Projectile {},
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub enum Particle {
    Blood { position: Vec3 },
    Laser { team: Team, matrix: Mat4 },
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub struct SpawnedParticle {
    spawned_tick: i32,
    particle: Particle,
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub struct WorldState {
    pub tick: i32,
    pub players: Vec<Player>,
    pub entities: Vec<Entity>,
    pub particles: Vec<Particle>,
}

fn main() {
    let state = WorldState {
        tick: 1,
        players: vec![
            Player {
                id: 1,
                name: "Mr Penis".to_string(),
                health: 100,
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
            },
            Player {
                id: 2,
                name: "John Cena".to_string(),
                health: 100,
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
            },
        ],
        entities: vec![],
        particles: vec![],
    };

    let bytes = rkyv::to_bytes::<Error>(&state).unwrap();

    let state2 = WorldState {
        tick: 1,
        players: vec![
            Player {
                id: 1,
                name: "Mr Penis".to_string(),
                health: 100,
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
            },
            Player {
                id: 2,
                name: "John Cena".to_string(),
                health: 100,
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
            },
        ],
        entities: vec![],
        particles: vec![Particle::Blood {
            position: Vec3::ZERO,
        }],
    };

    let bytes2 = rkyv::to_bytes::<Error>(&state2).unwrap();

    let print_bytes = |bytes: &[u8]| {
        println!(
            "Bytes ({}): {}",
            bytes.len(),
            bytes
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" ")
        );
    };

    //println!("Pre:\n{:#?}", state);

    print_bytes(&bytes);
    print_bytes(&bytes2);

    // calculate delta bytes using zstd
    let mut compressor = zstd::bulk::Compressor::with_dictionary(3, &bytes).unwrap();
    let delta = compressor.compress(&bytes2).unwrap();

    println!("Delta:");
    print_bytes(&delta);

    // Using `bytes` and `delta` get `new_bytes`
    let mut decompressor = zstd::bulk::Decompressor::with_dictionary(&bytes).unwrap();
    let new_bytes = decompressor.decompress(&delta, bytes2.len()).unwrap();

    let parsed_state = rkyv::access::<ArchivedWorldState, Error>(&new_bytes[..]).unwrap();

    println!("Post:\n{:#?}", parsed_state);
}
