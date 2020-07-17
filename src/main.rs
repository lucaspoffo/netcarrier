use netcarrier::*;
use serde::{Deserialize, Serialize};
use shipyard::*;
use bit_vec::BitVec;

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

    fn from(&self, _other: &Position) -> Option<DeltaPosition> {
        Some(DeltaPosition { x: 0, y: 0 })
    }

    fn apply(&self, _other: &Self::DeltaType) -> Position {
        Position { x: 0.0, y: 0.0 }
    }
}
generate_packet!(
    struct State {
        positions: Position,
    }
);

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

fn empty_snapshot() -> NetworkPacket {
    let entities_mask: BitVec<u32> = BitVec::new();
    let values: Vec<Position> = vec![];
    let network_bitmask = NetworkBitmask {
        entities_mask,
        values,
    };

    NetworkPacket {
        frame: 0,
        entities_id: vec![],
        positions: network_bitmask,
    }
}

fn main() {
    let snapshot = empty_snapshot();
    let mut state = empty_snapshot();
    // state.entities_id.push(2);
    // state.positions.add_value(Position::new(1.0, 1.0));

    let delta_state = state.from(&snapshot).unwrap();
    let applied_state = snapshot.apply(&delta_state);
    assert_eq!(applied_state, state);
    println!("{:?}", applied_state);
}
