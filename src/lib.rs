use std::sync::atomic::{AtomicU32, Ordering};
use std::fmt;

use rand::Rng;
use shipyard::*;
use serde::{Deserialize, Serialize};
use bit_vec::BitVec;
pub mod transport;

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

static NEXT_ID: AtomicU32 = AtomicU32::new(0);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct NetworkIdentifier {
	id: u32,
}

impl NetworkIdentifier {
	pub fn new() -> NetworkIdentifier {
		let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
		NetworkIdentifier { id }
	}
}

pub struct Game {
	pub frame: u32,
	pub world: World,
	added: Vec<u32>,
	removed: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkState {
	pub frame: u32,
	pub entities_id: Vec<u32>,
	pub added: Vec<u32>,
	pub removed: Vec<u32>,
	pub positions: NetworkBitmask<Position>,
	pub colors: NetworkBitmask<Color>
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub struct Color(pub [f32; 4]);

impl Color {
	pub fn random() -> Self {
		let mut rng = rand::thread_rng();
		Color([rng.gen_range(0.0, 1.0), rng.gen_range(0.0, 1.0), rng.gen_range(0.0, 1.0), 1.0])
	}
}

impl Game {
	pub fn new_empty() -> Game {
		Game { world: World::default(), added: vec![], removed: vec![], frame: 0 }
	}

	pub fn new() -> Game {
		let world = World::default();
		world.run(init_world);

		Game { world, added: vec![], removed: vec![], frame: 0 }
	}

	pub fn tick(&mut self) {
		self.frame += 1;
	}
	  
	pub fn encoded(&mut self) -> Vec<u8> {
		let mut entities_id = vec![];
		self.world.run(|net_ids: View<NetworkIdentifier>| {
			for net_id in net_ids.iter() {
				entities_id.push(net_id.id);	
			}
		});

		let net_state = NetworkState {
			frame: self.frame,
			entities_id: entities_id.clone(),
			positions: replicate::<Position>(&self.world, &entities_id),
			colors: replicate::<Color>(&self.world, &entities_id),
			added: self.added.clone(),
			removed: vec![] 
		};
		self.added.clear();
		bincode::serialize(&net_state).unwrap()
	}
}

fn init_world(mut entities: EntitiesViewMut, mut positions: ViewMut<Position>, mut velocities: ViewMut<Velocity>, mut net_ids: ViewMut<NetworkIdentifier>) {
	(0..5).for_each(|_| {
		let net_id = NetworkIdentifier::new();
		entities.add_entity(
			(&mut positions, &mut velocities, &mut net_ids),
			(Position::new(0.0, 0.0), Velocity::new(1.0, 1.0), net_id)
		);
	});
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkBitmask<T> {
	entities_mask: BitVec<u32>,
	pub values: Vec<T>
}

impl<T> NetworkBitmask<T> {
	pub fn masked_entities_id(&self, entities_ids: &[u32]) -> Vec<u32> {
		let mut masked_ids: Vec<u32> = vec![];
		self.entities_mask.iter().enumerate().for_each(|(i, bit)| {
			if bit == true {
				masked_ids.push(entities_ids[i]);
			}
		});
		masked_ids
	}
}

pub fn replicate<T: 'static + Sync + Send + fmt::Debug + Copy + Serialize>(world: &World, entities_id: &[u32]) -> NetworkBitmask<T> {
	let mut values = vec![];
	let mut entities_mask: BitVec<u32> = BitVec::from_elem(entities_id.len(), false);
	world.run(|storage: View<T>, net_ids: View<NetworkIdentifier>| {
		for (component, net_id) in (&storage, &net_ids).iter() {
			// TODO: use sparce set for the sweat 0(1) get instead of a find here
			let id_pos = entities_id.iter().position(|&x| x == net_id.id).expect("All network ids should be contained.");
			entities_mask.set(id_pos, true);
			values.push(*component);
		}
	});
	NetworkBitmask { entities_mask, values }
}

pub fn encoded<T: 'static + Sync + Send + fmt::Debug + Clone + Serialize>(world: &World) -> Vec<u8> {
    let mut encoded: Vec<u8> = vec![];
    world.run(|storage: View<T>, net_ids: View<NetworkIdentifier>| {
			let mut state: Vec<(&T, u32)> = vec![];
			for (component, net_id) in (&storage, &net_ids).iter() {
        state.push((component, net_id.id));
			}
			encoded = bincode::serialize(&state).unwrap();
		});
    encoded
}

pub fn deserilize<'de, T: Deserialize<'de>>(encoded: &'de [u8]) -> Vec<(T, u32)> {
    bincode::deserialize::<Vec<(T, u32)>>(encoded).unwrap()
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
