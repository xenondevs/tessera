use serde::Deserialize;
use tessera_derive::AllArray;
use ultraviolet::Vec3;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Deserialize, AllArray)]
#[repr(u8)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    North = 0,
    East = 1,
    South = 2,
    West = 3,
    Up = 4,
    Down = 5,
}

impl Direction {
    pub const fn unit(&self) -> Vec3 {
        match self {
            Direction::North => Vec3::new(0.0, 0.0, -1.0),
            Direction::East => Vec3::new(1.0, 0.0, 0.0),
            Direction::South => Vec3::new(0.0, 0.0, 1.0),
            Direction::West => Vec3::new(-1.0, 0.0, 0.0),
            Direction::Up => Vec3::new(0.0, 1.0, 0.0),
            Direction::Down => Vec3::new(0.0, -1.0, 0.0),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    X,
    Y,
    Z,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "i32")]
#[repr(u8)]
pub enum Quadrant {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

impl TryFrom<i32> for Quadrant {
    type Error = String;
    fn try_from(deg: i32) -> Result<Self, Self::Error> {
        Ok(match deg.rem_euclid(360) {
            0 => Self::R0,
            90 => Self::R90,
            180 => Self::R180,
            270 => Self::R270,
            _ => return Err(format!("Invalid rotation {deg}. (only 0/90/180/270 allowed")),
        })
    }
}
