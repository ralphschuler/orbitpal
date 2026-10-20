//! Canonical fixed-point coordinates. Monitor origins/scales are explicit data;
//! these helpers never enumerate displays, inspect windows or capture pixels.
use super::WorldError;
use serde::{de, Deserialize, Deserializer, Serialize};
pub const SCALE: u32 = 1_000_000;
pub const MAX_DIMENSION: f64 = 1_000_000.0;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Coordinate(u32);
impl Coordinate {
    pub fn new(value: u32) -> Result<Self, WorldError> {
        if value > SCALE {
            Err(WorldError::InvalidGeometry)
        } else {
            Ok(Self(value))
        }
    }
    pub fn value(self) -> u32 {
        self.0
    }
}
impl<'de> Deserialize<'de> for Coordinate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(u32::deserialize(d)?).map_err(de::Error::custom)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    x: Coordinate,
    y: Coordinate,
}
impl Point {
    pub fn new(x: u32, y: u32) -> Result<Self, WorldError> {
        Ok(Self {
            x: Coordinate::new(x)?,
            y: Coordinate::new(y)?,
        })
    }
    pub fn x(self) -> u32 {
        self.x.value()
    }
    pub fn y(self) -> u32 {
        self.y.value()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RectWire", into = "RectWire")]
pub struct Rect {
    min: Point,
    max: Point,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RectWire {
    min: Point,
    max: Point,
}
impl TryFrom<RectWire> for Rect {
    type Error = WorldError;
    fn try_from(v: RectWire) -> Result<Self, Self::Error> {
        Self::new(v.min, v.max)
    }
}
impl From<Rect> for RectWire {
    fn from(v: Rect) -> Self {
        Self {
            min: v.min,
            max: v.max,
        }
    }
}
impl Rect {
    pub fn new(min: Point, max: Point) -> Result<Self, WorldError> {
        let v = Self { min, max };
        v.validate()?;
        Ok(v)
    }
    pub fn full() -> Self {
        Self {
            min: Point {
                x: Coordinate(0),
                y: Coordinate(0),
            },
            max: Point {
                x: Coordinate(SCALE),
                y: Coordinate(SCALE),
            },
        }
    }
    pub fn validate(self) -> Result<(), WorldError> {
        if self.min.x() >= self.max.x()
            || self.min.y() >= self.max.y()
            || self.max.x() > SCALE
            || self.max.y() > SCALE
        {
            Err(WorldError::InvalidGeometry)
        } else {
            Ok(())
        }
    }
    pub fn contains(self, p: Point) -> bool {
        p.x() >= self.min.x()
            && p.x() <= self.max.x()
            && p.y() >= self.min.y()
            && p.y() <= self.max.y()
    }
    pub fn clamp(self, p: Point) -> Point {
        Point {
            x: Coordinate(p.x().clamp(self.min.x(), self.max.x())),
            y: Coordinate(p.y().clamp(self.min.y(), self.max.y())),
        }
    }
    pub fn center(self) -> Point {
        Point {
            x: Coordinate(self.min.x() + (self.max.x() - self.min.x()) / 2),
            y: Coordinate(self.min.y() + (self.max.y() - self.min.y()) / 2),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalSize {
    pub width: f64,
    pub height: f64,
}
impl LogicalSize {
    fn valid(self) -> bool {
        [self.width, self.height]
            .into_iter()
            .all(|n| n.is_finite() && n > 0.0 && n <= MAX_DIMENSION)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalPoint {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug)]
pub struct OutputGeometry {
    id: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
}
impl OutputGeometry {
    pub fn new(
        id: impl AsRef<str>,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<Self, WorldError> {
        let value = Self {
            id: id.as_ref().to_owned(),
            x,
            y,
            width,
            height,
            scale,
        };
        if value.id.is_empty()
            || value.id.len() > 512
            || value.id.chars().any(char::is_control)
            || !scale.is_finite()
            || !(0.25..=8.0).contains(&scale)
            || width == 0
            || height == 0
            || width > MAX_DIMENSION as u32
            || height > MAX_DIMENSION as u32
            || i64::from(x) + i64::from(width) > i64::from(i32::MAX)
            || i64::from(y) + i64::from(height) > i64::from(i32::MAX)
            || !value.logical_size().valid()
        {
            return Err(WorldError::InvalidGeometry);
        }
        Ok(value)
    }
    pub fn logical_size(&self) -> LogicalSize {
        LogicalSize {
            width: f64::from(self.width) / self.scale,
            height: f64::from(self.height) / self.scale,
        }
    }
    pub fn id(&self) -> &str {
        &self.id
    }
}
pub fn choose_output<'a>(
    outputs: &'a [OutputGeometry],
    preferred: &str,
) -> Option<&'a OutputGeometry> {
    outputs
        .iter()
        .find(|o| o.id == preferred)
        .or_else(|| outputs.iter().min_by(|a, b| a.id.cmp(&b.id)))
}
pub fn map_logical(
    point: Point,
    safe: Rect,
    view: LogicalSize,
    figure: LogicalSize,
    padding: f64,
) -> Result<LogicalPoint, WorldError> {
    safe.validate()?;
    if !view.valid() || !figure.valid() || !padding.is_finite() || !(0.0..=256.0).contains(&padding)
    {
        return Err(WorldError::InvalidGeometry);
    }
    let value = |n: u32, dimension: f64| f64::from(n) / f64::from(SCALE) * dimension;
    let left = value(safe.min.x(), view.width) + figure.width / 2.0 + padding;
    let right = value(safe.max.x(), view.width) - figure.width / 2.0 - padding;
    let top = value(safe.min.y(), view.height) + figure.height / 2.0 + padding;
    let bottom = value(safe.max.y(), view.height) - figure.height / 2.0 - padding;
    if left > right || top > bottom {
        return Err(WorldError::InvalidGeometry);
    }
    Ok(LogicalPoint {
        x: value(point.x(), view.width).clamp(left, right),
        y: value(point.y(), view.height).clamp(top, bottom),
    })
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}
pub fn map_physical(
    point: Point,
    safe: Rect,
    output: &OutputGeometry,
    figure: LogicalSize,
    padding: f64,
) -> Result<PhysicalPoint, WorldError> {
    let local = map_logical(point, safe, output.logical_size(), figure, padding)?;
    let convert = |origin: i32, position: f64| -> Result<i32, WorldError> {
        let value = f64::from(origin) + (position * output.scale).round();
        if value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
            return Err(WorldError::InvalidGeometry);
        }
        Ok(value as i32)
    };
    Ok(PhysicalPoint {
        x: convert(output.x, local.x)?,
        y: convert(output.y, local.y)?,
    })
}
