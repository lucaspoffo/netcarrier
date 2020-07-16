use netcarrier::generate_packet;
use rand::Rng;
use serde::{Deserialize, Serialize};

use netcarrier::Delta;

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub struct Color(pub [f32; 4]);

impl Color {
    pub fn random() -> Self {
        let mut rng = rand::thread_rng();
        Color([
            rng.gen_range(0.0, 1.0),
            rng.gen_range(0.0, 1.0),
            rng.gen_range(0.0, 1.0),
            1.0,
        ])
    }
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
            right: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Velocity {
    pub dx: f32,
    pub dy: f32,
}

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

impl Velocity {
    pub fn new(dx: f32, dy: f32) -> Velocity {
        Velocity { dx, dy }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Rectangle {
    pub width: f32,
    pub height: f32,
}

impl Rectangle {
    pub fn new(width: f32, height: f32) -> Rectangle {
        Rectangle { width, height }
    }
}

generate_packet!(struct Packet {
    positions: Position,
    velocities: Velocity,
    colors: Color,
    rectangles: Rectangle,
});

impl Delta for Position {
    type DeltaType = (f32, f32);

    fn from(&self, other: &Position) -> (f32, f32) {
        (other.x, other.y)
    }

    fn apply(&self, delta: &Self::DeltaType) -> Position {
        Position {
            x: delta.0,
            y: delta.1,
        }
    }
}

impl Delta for Velocity {
    type DeltaType = (f32, f32);

    fn from(&self, other: &Velocity) -> (f32, f32) {
        (other.dx, other.dy)
    }

    fn apply(&self, delta: &Self::DeltaType) -> Velocity {
        Velocity {
            dx: delta.0,
            dy: delta.1,
        }
    }
}

impl Delta for Rectangle {
    type DeltaType = (f32, f32);

    fn from(&self, other: &Rectangle) -> Self::DeltaType {
        (other.width, other.height)
    }

    fn apply(&self, delta: &Self::DeltaType) -> Rectangle {
        Rectangle {
            width: delta.0,
            height: delta.1,
        }
    }
}

impl Delta for Color {
    type DeltaType = [f32; 4];

    fn from(&self, other: &Color) -> Self::DeltaType {
        other.0
    }

    fn apply(&self, delta: &Self::DeltaType) -> Color {
        Color(*delta)
    }
}
