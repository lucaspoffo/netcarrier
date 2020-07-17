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
    type DeltaType = (i8, i8);

    fn from(&self, other: &Position) -> Option<Self::DeltaType> {
        let x = try_f32_to_i8(self.x - other.x);
        let y = try_f32_to_i8(self.y - other.y);
        if x.is_none() || y.is_none() {
            return None;
        }
        Some((x.unwrap(), y.unwrap()))
    }

    fn apply(&self, delta: &Self::DeltaType) -> Position {
        Position {
            x: self.x - delta.0 as f32,
            y: self.y - delta.1 as f32,
        }
    }
}

impl Delta for Velocity {
    type DeltaType = (i8, i8);

    fn from(&self, other: &Self) -> Option<Self::DeltaType> {
        let x = try_f32_to_i8(self.dx - other.dx);
        let y = try_f32_to_i8(self.dy - other.dy);
        if x.is_none() || y.is_none() {
            return None;
        }
        Some((x.unwrap(), y.unwrap()))
    }

    fn apply(&self, delta: &Self::DeltaType) -> Self {
        Velocity {
            dx: self.dx - delta.0 as f32,
            dy: self.dy - delta.1 as f32,
        }
    }
}

impl Delta for Rectangle {
    type DeltaType = ();

    fn from(&self, other: &Rectangle) -> Option<Self::DeltaType> {
        if other.width != self.width || other.height != self.height {
            None
        } else {
            Some(())
        }
    }

    fn apply(&self, _delta: &Self::DeltaType) -> Rectangle {
        Rectangle {
            width: self.width,
            height: self.height,
        }
    }
}

impl Delta for Color {
    type DeltaType = ();

    fn from(&self, other: &Color) -> Option<Self::DeltaType> {
        if self.0 != other.0 {
            Some(())
        } else {
            None
        }
    }

    fn apply(&self, _delta: &Self::DeltaType) -> Color {
        *self
    }
}

fn try_f32_to_i8(x: f32) -> Option<i8> {
    if x < (i8::MAX as f32) && x >= (i8::MIN as f32) {
        Some(x as i8)
    } else {
        None
    }
}
