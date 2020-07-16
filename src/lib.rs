use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};

use bit_vec::BitVec;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use shipyard::*;

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

impl<T> NetworkBitmask<T>
where
    T: Clone + Delta,
    T::DeltaType: Clone
{
    pub fn get_delta_bitmask(&self, delta_entities_id: &[u32], snapshot: &NetworkBitmask<T>, snapshot_entities_id: &[u32]) -> (NetworkBitmask<T>, NetworkBitmask<T::DeltaType>) {
        let ids_element = self.masked_entities_id(delta_entities_id);
        let snapshot_ids = snapshot.masked_entities_id(snapshot_entities_id);
        let mut element = vec![];
        let mut mask_element: BitVec<u32> = BitVec::from_elem(delta_entities_id.len(), false);
        let mut delta_element = vec![];
        let mut delta_mask_element: BitVec<u32> = BitVec::from_elem(delta_entities_id.len(), false);
        for (i, &id) in ids_element.iter().enumerate() {
            match snapshot_ids.iter().position(|&x| x == id) {
                Some(snapshot_index) => {
                    let snapshot_component = &snapshot.values[snapshot_index];
                    let current_component = &self.values[i];
                    let delta = snapshot_component.from(current_component);
                    delta_mask_element.set(i, true);
                    delta_element.push(delta);
                },
                None => {
                    let current_component = self.values[i].clone();
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

    pub fn apply_delta_bitmask(&self, snapshot_entities_id: &[u32], delta: &NetworkBitmask<T::DeltaType>, delta_entities_id: &[u32]) -> NetworkBitmask<T> {
        let snapshot_ids = self.masked_entities_id(&snapshot_entities_id);
        let ids_element = delta.masked_entities_id(&delta_entities_id);
        let mut element = vec![];
        for (i, &id) in ids_element.iter().enumerate() {
            let snapshot_index = snapshot_ids.iter().position(|&x| x == id).expect("All deltas ids must be in the snapshot");
            let snapshot_component = self.values[snapshot_index].clone();
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

// TODO: update tests to use new proc_macro, can't use it in here
// #[cfg(test)]
// #[allow(dead_code)]
// mod tests {
//     use super::*;
//     use serde::{Deserialize, Serialize};

//     #[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
//     pub struct Position {
//         pub x: f32,
//         pub y: f32,
//     }

//     impl Position {
//         pub fn new(x: f32, y: f32) -> Position {
//             Position { x, y }
//         }
//     }

//     #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
//     pub struct DeltaPosition {
//         pub x: u8,
//         pub y: u8,
//     }

//     impl Delta for Position {
//         type DeltaType = DeltaPosition;

//         fn from(&self, _other: &Position) -> DeltaPosition {
//             DeltaPosition { x: 0, y: 0 }
//         }

//         fn apply(&self, _other: &Self::DeltaType) -> Position {
//             Position { x: 0.0, y: 0.0 }
//         }
//     }

//     make_network_state!(positions: Position);

//     #[test]
//     fn join_network_bitmask() {
//         let mut a_mask: BitVec<u32> = BitVec::from_elem(2, false);
//         a_mask.set(0, true);
//         let a_values: Vec<u32> = vec![0];
//         let mut a = NetworkBitmask {
//             entities_mask: a_mask,
//             values: a_values,
//         };

//         let mut b_mask: BitVec<u32> = BitVec::from_elem(2, false);
//         b_mask.set(1, true);
//         let b_values: Vec<u32> = vec![1];
//         let b = NetworkBitmask {
//             entities_mask: b_mask,
//             values: b_values,
//         };

//         a.join(&b);
//         assert_eq!(a.entities_mask, BitVec::from_elem(2, true));
//         assert_eq!(a.values, vec![0, 1]);
//     }

//     fn setup_snapshot() -> NetworkPacket {
//         let entities_mask: BitVec<u32> = BitVec::from_elem(1, true);
//         let values = vec![Position::new(0.0, 0.0)];
//         let network_bitmask = NetworkBitmask {
//             entities_mask,
//             values,
//         };

//         NetworkPacket {
//             frame: 0,
//             entities_id: vec![1],
//             positions: network_bitmask,
//         }
//     }

//     #[test]
//     fn has_delta_component() {
//         let snapshot = setup_snapshot();
//         let mut state = setup_snapshot();
//         state.positions.values[0] = Position::new(1.0, 1.0);

//         let delta_state = state.from(&snapshot);
//         assert_eq!(delta_state.delta_positions.values.len(), 1);
//         assert_eq!(delta_state.positions.values.len(), 0);
//     }

//     #[test]
//     fn has_new_entity() {
//         let snapshot = setup_snapshot();
//         let mut state = setup_snapshot();
//         state.entities_id = vec![2];

//         let delta_state = state.from(&snapshot);
//         assert_eq!(delta_state.delta_positions.values.len(), 0);
//         assert_eq!(delta_state.positions.values.len(), 1);
//     }

//     #[test]
//     fn has_new_and_delta_entity() {
//         let snapshot = setup_snapshot();
//         let mut state = setup_snapshot();
//         state.entities_id.push(2);
//         state.positions.add_value(Position::new(1.0, 1.0));

//         let delta_state = state.from(&snapshot);
//         assert_eq!(delta_state.delta_positions.values.len(), 1);
//         assert_eq!(delta_state.positions.values.len(), 1);
//     }

//     #[test]
//     fn apply_delta_state() {
//         let snapshot = setup_snapshot();
//         let mut state = setup_snapshot();
//         state.entities_id.push(2);
//         state.positions.add_value(Position::new(1.0, 1.0));

//         let delta_state = state.from(&snapshot);
//         let applied_state = snapshot.apply(&delta_state);
//         assert_eq!(applied_state, state);
//     }
// }
