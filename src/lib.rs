#![recursion_limit = "2048"]
#![allow(dead_code)]
use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};

use bit_vec::BitVec;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use shipyard::*;

#[macro_use]
extern crate mashup;

pub mod transport;

pub use proc_macros::generate_packet;

#[doc(hidden)]
pub use ::serde;
#[doc(hidden)]
pub use ::shipyard;

static NEXT_ID: AtomicU32 = AtomicU32::new(0);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct NetworkIdentifier {
    pub id: u32,
}

pub trait CarrierPacket
where
	Self: Serialize + DeserializeOwned + Delta,
	Self::DeltaType: CarrierDeltaPacket
{
	fn frame(&self) -> u32;
	fn new(world: &World, frame: u32) -> Self;
	fn apply_state(&self, world: &World);
}

pub trait CarrierDeltaPacket: Serialize + DeserializeOwned {
	fn frame(&self) -> u32;
	fn snapshot_frame(&self) -> u32;
}

impl Default for NetworkIdentifier {
    fn default() -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        NetworkIdentifier { id }
    }
}

pub trait NetworkState {
    fn new(world: &World, frame: u32) -> Self;
    fn frame(&self) -> u32;
}

pub trait NetworkDeltaState {
    fn frame(&self) -> u32;
    fn snapshot_frame(&self) -> u32;
}

#[macro_export]
macro_rules! make_network_state {
	($($element: ident: $ty: ty),*) => {
		use $crate::{NetworkBitmask, NetworkIdentifier, replicate, NetworkState, NetworkDeltaState};
		use shipyard::*;
		use $crate::transport::NetworkIdMapping;
		use bit_vec::BitVec;

		mashup! {
			$(
				m["delta_" $element] = delta_ $element;
				m["delta_mask_" $element] = delta_mask_ $element;
				m["mask_" $element] = mask_ $element;
				m["network_" $element] = network_ $element;
				m["delta_network_" $element] = delta_network_ $element;
				m["ids_" $element] = ids_ $element;
				m["snapshot_ids_" $element] = snapshot_ids_ $element;
			)*
		}

		#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
		pub struct NetworkPacket {
			pub frame: u32,
			pub entities_id: Vec<u32>,
			$(pub $element: NetworkBitmask<$ty>),*
		}

		impl NetworkState for NetworkPacket {
			fn frame(&self) -> u32 {
				self.frame
			}

			fn new(world: &World, frame: u32) -> Self {
				let mut entities_id = vec![];
				world.run(|net_ids: View<NetworkIdentifier>| {
					for net_id in net_ids.iter() {
						entities_id.push(net_id.id);
					}
				});

				NetworkPacket {
					frame,
					entities_id: entities_id.clone(),
					$($element: replicate::<$ty>(&world, &entities_id)),*
				}
			}
		}

		impl NetworkPacket {
			pub fn apply_state(&self, mut all_storages: AllStoragesViewMut) {
				let mut removed_entities: Vec<EntityId> = vec![];
				{
					let mut entities = all_storages.borrow::<EntitiesViewMut>();
					let mut net_id_mapping = all_storages.borrow::<UniqueViewMut<NetworkIdMapping>>();
					$(let mut $element = all_storages.borrow::<ViewMut<$ty>>());*;
					// Create new ids
					for entity_id in &self.entities_id {
						if !net_id_mapping.0.contains_key(&entity_id) {
							let entity = entities.add_entity((), ());
							net_id_mapping.0.insert(*entity_id, entity);
						}
					}

					//Remove entities
					for net_id in net_id_mapping.0.keys() {
						if !self.entities_id.contains(net_id) {
							removed_entities.push(net_id_mapping.0[net_id]);
						}
					}

					// For each component type updates/creates value
					$({
						let masked_entities_ids = self.$element.masked_entities_id(&self.entities_id);
						for (i, component) in self.$element.values.iter().enumerate() {
							let net_id = &masked_entities_ids[i];
							if let Some(&id) = net_id_mapping.0.get(net_id) {
								if !$element.contains(id) {
								 	entities.add_component(&mut $element, *component, id);
								} else {
									$element[id] = *component;
								}
							}
						}
					})*
				}
				for entity_id in removed_entities {
					all_storages.delete(entity_id);
				}
			}
		}

		m! {
			#[derive(Debug, Serialize, Deserialize)]
			pub struct NetworkDeltaPacket {
				pub frame: u32,
				pub snapshot_frame: u32,
				pub entities_id: Vec<u32>,
				$(pub $element: NetworkBitmask<$ty>,)*
				$(pub "delta_" $element: NetworkBitmask<<$ty as Delta>::DeltaType>),*
			}
		}

		impl NetworkDeltaState for NetworkDeltaPacket {
			fn frame(&self) -> u32 {
				self.frame
			}

			fn snapshot_frame(&self) -> u32 {
				self.snapshot_frame
			}
		}

		m! {
			impl Delta for NetworkPacket {
				type DeltaType = NetworkDeltaPacket;

				fn from(&self, snapshot: &NetworkPacket) -> Self::DeltaType {
					let entities_len = self.entities_id.len();
					//TODO: we should have a sparce set so from the id we can get the component, instead of a find
					$(
						let "ids_" $element = self.$element.masked_entities_id(&self.entities_id);
						let "snapshot_ids_" $element = snapshot.$element.masked_entities_id(&snapshot.entities_id);
						let mut $element = vec![];
						let mut "mask_" $element: BitVec<u32> = BitVec::from_elem(entities_len, false);
						let mut "delta_" $element = vec![];
						let mut "delta_mask_" $element: BitVec<u32> = BitVec::from_elem(entities_len, false);
						for (i, &id) in "ids_" $element.iter().enumerate() {
							match "snapshot_ids_" $element.iter().position(|&x| x == id) {
								Some(snapshot_index) => {
									let snapshot_component = snapshot.$element.values[snapshot_index];
									let current_component = self.$element.values[i];
									let delta = snapshot_component.from(&current_component);
									"delta_mask_" $element.set(i, true);
            			"delta_" $element.push(delta);
								},
								None => {
									let current_component = self.$element.values[i];
									"mask_" $element.set(i, true);
            			$element.push(current_component);
								}
							}
						}

						let "network_" $element = NetworkBitmask {
							values: $element,
							entities_mask: "mask_" $element,
						};
						let "delta_network_" $element = NetworkBitmask {
							values: "delta_" $element,
							entities_mask: "delta_mask_" $element,
						};
					)*

					NetworkDeltaPacket {
						frame: self.frame(),
						snapshot_frame: snapshot.frame(),
						entities_id: self.entities_id.clone(),
						$($element: "network_" $element,)*
						$("delta_" $element: "delta_network_" $element,)*
					}
				}

				fn apply(&self, delta: &Self::DeltaType) -> Self {
					$(
						let "snapshot_ids_" $element = self.$element.masked_entities_id(&self.entities_id);
						let "ids_" $element = delta."delta_" $element.masked_entities_id(&delta.entities_id);
						let mut $element = vec![];
						for (i, &id) in "ids_" $element.iter().enumerate() {
							let snapshot_index = "snapshot_ids_" $element.iter().position(|&x| x == id).expect("All deltas ids must be in the snapshot");
							let snapshot_component = self.$element.values[snapshot_index].clone();
							let delta_component = delta."delta_" $element.values[i].clone();
							let component = snapshot_component.apply(&delta_component);
							$element.push(component);
						}

						let mut "network_" $element = NetworkBitmask {
							values: $element,
							entities_mask: delta."delta_" $element.entities_mask.clone(),
						};
						// Append full networkbitmask with delta networkbitmask
						"network_" $element.join(&delta.$element);
					)*

					NetworkPacket {
						frame: delta.frame,
						entities_id: delta.entities_id.clone(),
						$($element: "network_" $element),*
					}
				}
			}
		}
	}
}

// TODO: make from return Result, sometimes we can't retrieve an delta
pub trait Delta {
    type DeltaType: Serialize + DeserializeOwned;

    fn from(&self, other: &Self) -> Self::DeltaType;
    fn apply(&self, other: &Self::DeltaType) -> Self;
}

pub struct NetworkController {
    pub frame: u32,
    snapshot_frequency: u32,
}

impl NetworkController {
    pub fn new(snapshot_frequency: u32) -> Self {
        NetworkController {
            frame: 0,
            snapshot_frequency,
        }
    }

    pub fn tick(&mut self) {
        self.frame += 1;
    }

    pub fn is_snapshot_frame(&self) -> bool {
        (self.frame % self.snapshot_frequency) == 0
    }
}

// TODO: review attributes visibilities
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct NetworkBitmask<T> {
    pub entities_mask: BitVec<u32>,
    pub values: Vec<T>,
}

impl<T> NetworkBitmask<T>
where
    T: Clone,
{
    pub fn masked_entities_id(&self, entities_ids: &[u32]) -> Vec<u32> {
        let mut masked_ids: Vec<u32> = vec![];
        self.entities_mask.iter().enumerate().for_each(|(i, bit)| {
            if bit {
                masked_ids.push(entities_ids[i]);
            }
        });
        masked_ids
    }

    //TODO: this should not be pub (test purposes only?, maybe pass to NetworkState)
    pub fn add_value(&mut self, value: T) {
        self.values.push(value);
        self.entities_mask.push(true);
    }

    // TODO: should this be &self, and return new NetworkBitmask<T>?
    pub fn join(&mut self, other: &NetworkBitmask<T>) {
        // Todo: Both should have the same length (assert? return Result?)
        let len = self.entities_mask.len();
        assert_eq!(len, other.entities_mask.len());
        let mut entities_mask: BitVec<u32> = BitVec::from_elem(len, false);
        let mut values = vec![];
        let mut self_index = 0;
        let mut other_index = 0;
        for i in 0..len {
            if self.entities_mask.get(i).unwrap() {
                entities_mask.set(i, true);
                values.push(self.values[self_index].clone());
                self_index += 1;
            } else if other.entities_mask.get(i).unwrap() {
                entities_mask.set(i, true);
                values.push(other.values[other_index].clone());
                other_index += 1;
            }
        }
        self.entities_mask = entities_mask;
        self.values = values;
    }
}

pub fn replicate<T: 'static + Sync + Send + fmt::Debug + Copy + Serialize>(
    world: &World,
    entities_id: &[u32],
) -> NetworkBitmask<T> {
    let mut values = vec![];
    let mut entities_mask: BitVec<u32> = BitVec::from_elem(entities_id.len(), false);
    world.run(|storage: View<T>, net_ids: View<NetworkIdentifier>| {
        for (component, net_id) in (&storage, &net_ids).iter() {
            // TODO: use sparce set for the sweat O(1) get instead of a find here
            let id_pos = entities_id
                .iter()
                .position(|&x| x == net_id.id)
                .expect("All network ids should be contained.");
            entities_mask.set(id_pos, true);
            values.push(*component);
        }
    });
    NetworkBitmask {
        entities_mask,
        values,
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
    pub struct Position {
        pub x: f32,
        pub y: f32,
    }

    impl Position {
        pub fn new(x: f32, y: f32) -> Position {
            Position { x, y }
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    pub struct DeltaPosition {
        pub x: u8,
        pub y: u8,
    }

    impl Delta for Position {
        type DeltaType = DeltaPosition;

        fn from(&self, _other: &Position) -> DeltaPosition {
            DeltaPosition { x: 0, y: 0 }
        }

        fn apply(&self, _other: &Self::DeltaType) -> Position {
            Position { x: 0.0, y: 0.0 }
        }
    }

    make_network_state!(positions: Position);

    #[test]
    fn join_network_bitmask() {
        let mut a_mask: BitVec<u32> = BitVec::from_elem(2, false);
        a_mask.set(0, true);
        let a_values: Vec<u32> = vec![0];
        let mut a = NetworkBitmask {
            entities_mask: a_mask,
            values: a_values,
        };

        let mut b_mask: BitVec<u32> = BitVec::from_elem(2, false);
        b_mask.set(1, true);
        let b_values: Vec<u32> = vec![1];
        let b = NetworkBitmask {
            entities_mask: b_mask,
            values: b_values,
        };

        a.join(&b);
        assert_eq!(a.entities_mask, BitVec::from_elem(2, true));
        assert_eq!(a.values, vec![0, 1]);
    }

    fn setup_snapshot() -> NetworkPacket {
        let entities_mask: BitVec<u32> = BitVec::from_elem(1, true);
        let values = vec![Position::new(0.0, 0.0)];
        let network_bitmask = NetworkBitmask {
            entities_mask,
            values,
        };

        NetworkPacket {
            frame: 0,
            entities_id: vec![1],
            positions: network_bitmask,
        }
    }

    #[test]
    fn has_delta_component() {
        let snapshot = setup_snapshot();
        let mut state = setup_snapshot();
        state.positions.values[0] = Position::new(1.0, 1.0);

        let delta_state = state.from(&snapshot);
        assert_eq!(delta_state.delta_positions.values.len(), 1);
        assert_eq!(delta_state.positions.values.len(), 0);
    }

    #[test]
    fn has_new_entity() {
        let snapshot = setup_snapshot();
        let mut state = setup_snapshot();
        state.entities_id = vec![2];

        let delta_state = state.from(&snapshot);
        assert_eq!(delta_state.delta_positions.values.len(), 0);
        assert_eq!(delta_state.positions.values.len(), 1);
    }

    #[test]
    fn has_new_and_delta_entity() {
        let snapshot = setup_snapshot();
        let mut state = setup_snapshot();
        state.entities_id.push(2);
        state.positions.add_value(Position::new(1.0, 1.0));

        let delta_state = state.from(&snapshot);
        assert_eq!(delta_state.delta_positions.values.len(), 1);
        assert_eq!(delta_state.positions.values.len(), 1);
    }

    #[test]
    fn apply_delta_state() {
        let snapshot = setup_snapshot();
        let mut state = setup_snapshot();
        state.entities_id.push(2);
        state.positions.add_value(Position::new(1.0, 1.0));

        let delta_state = state.from(&snapshot);
        let applied_state = snapshot.apply(&delta_state);
        assert_eq!(applied_state, state);
    }
}

//TODO: put this on NetworkBitmask impl
pub fn get_delta_bitmask<T: Delta + Clone>(delta: &NetworkBitmask<T>, snapshot: &NetworkBitmask<T>, entities_id: &[u32], snapshot_entities_id: &[u32]) -> (NetworkBitmask<T>, NetworkBitmask<T::DeltaType>) {
    let ids_element = delta.masked_entities_id(entities_id);
    let snapshot_ids = snapshot.masked_entities_id(snapshot_entities_id);
    let mut element = vec![];
    let mut mask_element: BitVec<u32> = BitVec::from_elem(entities_id.len(), false);
    let mut delta_element = vec![];
    let mut delta_mask_element: BitVec<u32> = BitVec::from_elem(entities_id.len(), false);
    for (i, &id) in ids_element.iter().enumerate() {
        match snapshot_ids.iter().position(|&x| x == id) {
            Some(snapshot_index) => {
                let snapshot_component = &snapshot.values[snapshot_index];
                let current_component = &delta.values[i];
                let delta = snapshot_component.from(current_component);
                delta_mask_element.set(i, true);
                delta_element.push(delta);
            },
            None => {
                let current_component = delta.values[i].clone();
                mask_element.set(i, true);
                element.push(current_component);
            }
        }
    }
    
    let network_element = NetworkBitmask {
        values: element,
        entities_mask: mask_element,
    };
    let delta_network_element = NetworkBitmask {
        values: delta_element,
        entities_mask: delta_mask_element,
    };

    (network_element, delta_network_element)
}

pub fn apply_delta_bitmask<T>(delta: &NetworkBitmask<T::DeltaType>, snapshot: &NetworkBitmask<T>, entities_id: &[u32], snapshot_entities_id: &[u32]) -> NetworkBitmask<T> 
where T: Delta + Clone, T::DeltaType: Clone {
    
    let snapshot_ids = snapshot.masked_entities_id(&snapshot_entities_id);
    let ids_element = delta.masked_entities_id(&entities_id);
    let mut element = vec![];
    for (i, &id) in ids_element.iter().enumerate() {
        let snapshot_index = snapshot_ids.iter().position(|&x| x == id).expect("All deltas ids must be in the snapshot");
        let snapshot_component = snapshot.values[snapshot_index].clone();
        let delta_component = delta.values[i].clone();
        let component = snapshot_component.apply(&delta_component);
        element.push(component);
    }
    
    let network_element = NetworkBitmask {
        values: element,
        entities_mask: delta.entities_mask.clone(),
    };
    // Append full networkbitmask with delta networkbitmask
    // network_element.join(&delta.$element);
    network_element
}
