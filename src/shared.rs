use super::make_network_state;
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::Delta;

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

make_network_state!(positions: Position, colors: Color, rectangles: Rectangle);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DeltaPosition {
    pub x: u8,
    pub y: u8
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

impl Delta for Velocity {
    type DeltaType = DeltaPosition;

    fn from(&self, _other: &Velocity) -> DeltaPosition {
        DeltaPosition { x: 0, y: 0 }
    } 

    fn apply(&self, _other: &Self::DeltaType) -> Velocity {
        Velocity { dx: 0.0, dy: 0.0 }
    }
}

impl Delta for Rectangle {
    type DeltaType = DeltaPosition;

    fn from(&self, _other: &Rectangle) -> DeltaPosition {
        DeltaPosition { x: 0, y: 0 }
    }

    fn apply(&self, _other: &Self::DeltaType) -> Rectangle {
        Rectangle { width: 0.0, height: 0.0 }
    }
}

impl Delta for Color {
    type DeltaType = DeltaPosition;

    fn from(&self, _other: &Color) -> DeltaPosition {
        DeltaPosition { x: 0, y: 0 }
    }

    fn apply(&self, _other: &Self::DeltaType) -> Color {
        Color([0.0, 0.0, 0.0, 0.0])
    }
}
