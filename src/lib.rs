use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};

use bit_vec::BitVec;
use serde::{Deserialize, Serialize};
use shipyard::*;

pub mod shared;
pub mod transport;



static NEXT_ID: AtomicU32 = AtomicU32::new(0);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct NetworkIdentifier {
    id: u32,
}

impl Default for NetworkIdentifier {
    fn default() -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        NetworkIdentifier { id }
    }
}

pub trait NetworkFrame {
	fn frame(&self) -> u32;
}

#[macro_export]
macro_rules! make_network_state {

	($($element: ident: $ty: ty),*) => {
		use $crate::{NetworkBitmask, NetworkIdentifier, replicate, NetworkFrame};
		use shipyard::*;
		use $crate::transport::NetworkIdMapping;

		#[derive(Debug, Serialize, Deserialize)]
		pub struct NetworkState {
			pub frame: u32,
			pub entities_id: Vec<u32>,
			$(pub $element: NetworkBitmask<$ty>),*
		}

		impl NetworkFrame for NetworkState {
			fn frame(&self) -> u32 {
				self.frame
			}
		}

		impl NetworkState {
			pub fn new(world: &World, frame: u32) -> Self {
				let mut entities_id = vec![];
				world.run(|net_ids: View<NetworkIdentifier>| {
					for net_id in net_ids.iter() {
						entities_id.push(net_id.id);
					}
				});

				NetworkState {
					frame,
					entities_id: entities_id.clone(),
					$($element: replicate::<$ty>(&world, &entities_id)),*
				}
			}

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
	}
}

#[derive(Default)]
pub struct NetworkController {
    pub frame: u32,
}

impl NetworkController {
    pub fn tick(&mut self) {
        self.frame += 1;
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkBitmask<T> {
    entities_mask: BitVec<u32>,
    pub values: Vec<T>,
}

impl<T> NetworkBitmask<T> {
    pub fn masked_entities_id(&self, entities_ids: &[u32]) -> Vec<u32> {
        let mut masked_ids: Vec<u32> = vec![];
        self.entities_mask.iter().enumerate().for_each(|(i, bit)| {
            if bit {
                masked_ids.push(entities_ids[i]);
            }
        });
        masked_ids
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
