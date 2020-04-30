use std::sync::atomic::{AtomicUsize, Ordering};
use std::fmt;

use shipyard::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Velocity { 
	pub dx: f32, 
	pub dy: f32 
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Position { 
	pub x: f32, 
	pub y: f32 
}

impl Position {
    pub fn new(x: f32, y: f32) -> Position {
        Position { x, y }
    }
}

impl Velocity {
    pub fn new(dx: f32, dy: f32) -> Velocity {
        Velocity { dx, dy }
    }
}

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct NetworkIdentifier {
	id: usize,
}

impl NetworkIdentifier {
	pub fn new() -> NetworkIdentifier {
		let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
		NetworkIdentifier { id }
	}
}

pub struct Game {
	pub world: World,
	added: Vec<usize>,
	removed: Vec<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkState {
	pub added: Vec<usize>,
	pub removed: Vec<usize>,
	pub positions: Vec<(Position, usize)>,
}

impl Game {
	pub fn new_empty() -> Game {
		Game { world: World::default(), added: vec![], removed: vec![] }
	}

	pub fn new() -> Game {
		let world = World::default();
		let mut added = vec![];
		world.run::<(EntitiesMut, &mut Position, &mut Velocity, &mut NetworkIdentifier), _, _>(|(mut entities, mut positions, mut velocities, mut net_identifiers)| {
			(0..5).for_each(|_| {
				let net_id = NetworkIdentifier::new();
				entities.add_entity(
					(&mut positions, &mut velocities, &mut net_identifiers),
					(Position::new(0.0, 0.0), Velocity::new(1.0, 1.0), net_id)
				);
				added.push(net_id.id);
			});

			(0..5).for_each(|_| {
				entities.add_entity(
					(&mut positions, &mut velocities),
					(Position::new(0.0, 0.0), Velocity::new(1.0, 1.0))
				);
			});
		});

		Game { world, added: added.clone(), removed: vec![] }
	}
	  
	pub fn encoded(&mut self) -> Vec<u8> {
		// self.world.run::<&Position, _, _>(|positions| {
        // 	positions.iter().for_each(|pos| {
        //     	println!("{:?}", pos);
        // 	});
    	// });
		// println!("{:?}", replicate::<Position>(&self.world));
		let net_state = NetworkState { 
			positions: replicate::<Position>(&self.world),
			added: self.added.clone(),
			removed: vec![] 
		};
		self.added.clear();
		bincode::serialize(&net_state).unwrap()
	}
}

pub fn replicate<T: 'static + Sync + Send + fmt::Debug + Copy + Serialize>(world: &World) -> Vec<(T, usize)> {
    let mut state: Vec<(T, usize)> = vec![];
    world.run::<(&T, &NetworkIdentifier), _, _>(|(storage, net_identifiers)| {
        (&storage, &net_identifiers).iter().for_each(|(component, net_identifier)| {
            // println!("{:?}, {:?}", component, net_identifier);
            state.push((*component, net_identifier.id));
			// println!("{:?}", state);
		});
	});
	state
}

pub fn encoded<T: 'static + Sync + Send + fmt::Debug + Clone + Serialize>(world: &World) -> Vec<u8> {
    let mut encoded: Vec<u8> = vec![];
    world.run::<(&T, &NetworkIdentifier), _, _>(|(storage, net_identifiers)| {
        let mut state: Vec<(&T, usize)> = vec![];
        (&storage, &net_identifiers).iter().for_each(|(component, net_identifier)| {
            // println!("{:?}, {:?}", component, net_identifier);
            state.push((component, net_identifier.id));
        });
        encoded = bincode::serialize(&state).unwrap();
    });
    encoded
}

pub fn deserilize<'de, T: Deserialize<'de>>(encoded: &'de [u8]) -> Vec<(T, usize)> {
    bincode::deserialize::<Vec<(T, usize)>>(encoded).unwrap()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientState {
	pub up: bool,
	pub down: bool,
	pub left: bool,
	pub right: bool,
}

impl Default for ClientState {
	fn default() -> ClientState {
		ClientState {
			up: false,
			down: false,
			left: false,
			right: false
		}
	}
}
