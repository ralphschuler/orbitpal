//! Ephemeral geometry mapping only. No OS query, capture, input or window mutation.
use crate::experience::{Preferences, Preset};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}
#[derive(Clone, Debug)]
pub struct GeometrySample {
    pub source_generation: u64,
    pub display_id: String,
    pub at_ms: u64,
    pub work_area: Rect,
    pub window_top: Option<Rect>,
    pub cursor: Option<(i32, i32)>,
}
#[derive(Default)]
pub struct GeometryLease {
    sample: Option<GeometrySample>,
}
impl GeometryLease {
    /// Native verified provider only; a renderer cannot submit or persist geometry.
    pub fn observe(&mut self, sample: GeometrySample) -> bool {
        let valid = sample.source_generation > 0
            && !sample.display_id.is_empty()
            && sample.display_id.len() <= 512
            && sample.work_area.width > 0
            && sample.work_area.height > 0
            && sample.work_area.width <= 65536
            && sample.work_area.height <= 65536
            && sample.window_top.is_none_or(|r| {
                r.width > 0 && r.height > 0 && r.width <= 65536 && r.height <= 65536
            });
        if valid {
            self.sample = Some(sample);
        } else {
            self.sample = None;
        }
        valid
    }
    pub fn invalidate(&mut self) {
        self.sample = None;
    }
    pub fn anchors(
        &self,
        generation: u64,
        display: &str,
        now: u64,
        prefs: &Preferences,
        paused: bool,
    ) -> Vec<Point> {
        if paused || prefs.validate().is_err() {
            return vec![];
        }
        let Some(s) = &self.sample else { return vec![] };
        if generation != s.source_generation
            || display != s.display_id
            || now < s.at_ms
            || now - s.at_ms > 1500
        {
            return vec![];
        }
        let map = |x: i64, y: i64| -> Option<Point> {
            let r = s.work_area;
            let rx = x - i64::from(r.x);
            let ry = y - i64::from(r.y);
            if rx < 0 || ry < 0 || rx > i64::from(r.width) || ry > i64::from(r.height) {
                return None;
            }
            Some(Point {
                x: (rx * 1_000_000 / i64::from(r.width)) as u32,
                y: (ry * 1_000_000 / i64::from(r.height)) as u32,
            })
        };
        let mut anchors = vec![];
        if prefs.desktop_anchors {
            if let Some(r) = s.window_top {
                if let Some(p) = map(i64::from(r.x) + i64::from(r.width) / 2, i64::from(r.y)) {
                    anchors.push(p)
                }
            }
        }
        if prefs.cursor_play && prefs.preset == Preset::Stream {
            if let Some((x, y)) = s.cursor {
                if let Some(p) = map(i64::from(x), i64::from(y)) {
                    anchors.push(p)
                }
            }
        }
        anchors
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> GeometrySample {
        GeometrySample {
            source_generation: 2,
            display_id: "left".into(),
            at_ms: 100,
            work_area: Rect {
                x: -1920,
                y: -200,
                width: 1920,
                height: 1080,
            },
            window_top: Some(Rect {
                x: -1200,
                y: 100,
                width: 400,
                height: 600,
            }),
            cursor: Some((-960, 340)),
        }
    }
    #[test]
    fn physical_mapping_handles_negative_monitor_origins() {
        let mut g = GeometryLease::default();
        assert!(g.observe(sample()));
        let p = Preferences {
            desktop_anchors: true,
            ..Default::default()
        };
        assert_eq!(
            g.anchors(2, "left", 100, &p, false),
            vec![Point {
                x: 479166,
                y: 277777
            }]
        );
    }
    #[test]
    fn stale_display_generation_and_time_fail_closed() {
        let mut g = GeometryLease::default();
        g.observe(sample());
        let p = Preferences {
            desktop_anchors: true,
            ..Default::default()
        };
        for (gen, display, now) in [
            (3, "left", 100),
            (2, "right", 100),
            (2, "left", 1601),
            (2, "left", 99),
        ] {
            assert!(g.anchors(gen, display, now, &p, false).is_empty());
        }
        g.invalidate();
        assert!(g.anchors(2, "left", 100, &p, false).is_empty());
    }
    #[test]
    fn cursor_is_opt_in_and_quiet_in_focus() {
        let mut g = GeometryLease::default();
        g.observe(sample());
        let mut p = Preferences {
            cursor_play: true,
            ..Default::default()
        };
        assert!(g.anchors(2, "left", 100, &p, false).is_empty());
        p.preset = Preset::Stream;
        assert_eq!(
            g.anchors(2, "left", 100, &p, false),
            vec![Point {
                x: 500000,
                y: 500000
            }]
        );
        assert!(g.anchors(2, "left", 100, &p, true).is_empty());
    }
    #[test]
    fn uniformly_scaled_physical_geometry_maps_identically() {
        let mut a = GeometryLease::default();
        a.observe(sample());
        let mut s = sample();
        s.work_area = Rect {
            x: -3840,
            y: -400,
            width: 3840,
            height: 2160,
        };
        s.window_top = Some(Rect {
            x: -2400,
            y: 200,
            width: 800,
            height: 1200,
        });
        let mut b = GeometryLease::default();
        b.observe(s);
        let p = Preferences {
            desktop_anchors: true,
            ..Default::default()
        };
        assert_eq!(
            a.anchors(2, "left", 100, &p, false),
            b.anchors(2, "left", 100, &p, false)
        );
    }
}
