//! A monitor, and the pure mappings between the desktop and one monitor.

use crate::geometry::{
    LocalPoint, LocalRect, LogicalPoint, LogicalRect, PhysicalPoint, PhysicalRect,
};

/// DPI at which one logical pixel is one physical pixel (100% scaling).
const BASE_DPI: u32 = 96;

/// Identifies a monitor across enumerations.
///
/// For a live monitor this is its device interface path (which names the
/// panel and the connector, and survives reboots and re-arrangement), or the
/// GDI device name (`\\.\DISPLAY1`) when Windows reports no interface path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MonitorId(String);

impl MonitorId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One monitor of the desktop, as it was when it was enumerated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    id: MonitorId,
    bounds: PhysicalRect,
    dpi: u32,
    primary: bool,
}

impl Monitor {
    /// A monitor covering `bounds` at `dpi` (96 is 100% scaling). A `dpi` of
    /// zero is read as 96, so the scale factor is never zero.
    pub fn new(id: MonitorId, bounds: PhysicalRect, dpi: u32, primary: bool) -> Self {
        let dpi = if dpi == 0 { BASE_DPI } else { dpi };
        Self {
            id,
            bounds,
            dpi,
            primary,
        }
    }

    pub fn id(&self) -> &MonitorId {
        &self.id
    }

    /// The whole monitor (not just its work area) in physical pixels.
    pub fn bounds(&self) -> PhysicalRect {
        self.bounds
    }

    /// Effective DPI: 96 at 100%, 120 at 125%, 144 at 150%.
    pub fn dpi(&self) -> u32 {
        self.dpi
    }

    /// Physical pixels per logical pixel: 1.0 at 100%, 1.25 at 125%.
    pub fn scale(&self) -> f64 {
        f64::from(self.dpi) / f64::from(BASE_DPI)
    }

    pub fn is_primary(&self) -> bool {
        self.primary
    }

    /// Size of an overlay covering this monitor, in logical pixels. A
    /// 1920x1200 monitor at 125% is 1536x960.
    pub fn logical_size(&self) -> (i32, i32) {
        (
            self.to_logical(self.bounds.width),
            self.to_logical(self.bounds.height),
        )
    }

    /// A desktop point relative to this monitor's corner. The result lies
    /// outside the monitor when the point does.
    pub fn point_to_local(&self, point: PhysicalPoint) -> LocalPoint {
        LocalPoint::new(
            point.x.saturating_sub(self.bounds.x),
            point.y.saturating_sub(self.bounds.y),
        )
    }

    pub fn point_to_physical(&self, point: LocalPoint) -> PhysicalPoint {
        PhysicalPoint::new(
            point.x.saturating_add(self.bounds.x),
            point.y.saturating_add(self.bounds.y),
        )
    }

    /// The part of a desktop rectangle that is on this monitor, relative to
    /// its corner. `None` when no part of it is.
    pub fn rect_to_local(&self, rect: PhysicalRect) -> Option<LocalRect> {
        let visible = rect.intersection(&self.bounds)?;
        let origin = self.point_to_local(visible.origin());
        Some(LocalRect::new(
            origin.x,
            origin.y,
            visible.width,
            visible.height,
        ))
    }

    pub fn rect_to_physical(&self, rect: LocalRect) -> PhysicalRect {
        let origin = self.point_to_physical(rect.origin());
        PhysicalRect::new(origin.x, origin.y, rect.width, rect.height)
    }

    /// A point on this monitor's overlay as a desktop point.
    pub fn logical_point_to_physical(&self, point: LogicalPoint) -> PhysicalPoint {
        self.point_to_physical(LocalPoint::new(
            self.to_scaled(point.x),
            self.to_scaled(point.y),
        ))
    }

    /// A rectangle on this monitor's overlay as a desktop rectangle.
    pub fn logical_rect_to_physical(&self, rect: LogicalRect) -> PhysicalRect {
        // Edges, not corner and size: see `rect_to_logical`
        self.rect_to_physical(LocalRect::from_edges(
            self.to_scaled(rect.x),
            self.to_scaled(rect.y),
            self.to_scaled(rect.right()),
            self.to_scaled(rect.bottom()),
        ))
    }

    /// A desktop rectangle on this monitor's overlay, clipped to the overlay.
    /// `None` when the rectangle is not on this monitor at all.
    pub fn rect_to_logical(&self, rect: PhysicalRect) -> Option<LogicalRect> {
        let (width, height) = self.logical_size();
        // Edges are converted rather than the corner and size, so rounding
        // can never leave a one-pixel gap between a window frame and the
        // selection made from it
        let mapped = LogicalRect::from_edges(
            self.to_logical(rect.x.saturating_sub(self.bounds.x)),
            self.to_logical(rect.y.saturating_sub(self.bounds.y)),
            self.to_logical(rect.right().saturating_sub(self.bounds.x)),
            self.to_logical(rect.bottom().saturating_sub(self.bounds.y)),
        );
        mapped.intersection(&LogicalRect::new(0, 0, width, height))
    }

    fn to_logical(&self, physical: i32) -> i32 {
        round(f64::from(physical) / self.scale())
    }

    fn to_scaled(&self, logical: i32) -> i32 {
        round(f64::from(logical) * self.scale())
    }
}

/// Rounds half away from zero, as the overlay's layout does.
fn round(value: f64) -> i32 {
    // `as` saturates at the i32 range, which no desktop coordinate reaches
    value.round() as i32
}

/// The monitor whose bounds contain `point`. `None` when the point is in a
/// gap between monitors or off the desktop.
pub fn monitor_at(monitors: &[Monitor], point: PhysicalPoint) -> Option<&Monitor> {
    monitors
        .iter()
        .find(|monitor| monitor.bounds.contains(point))
}

/// The monitor under `point`, or the primary monitor when there is none
/// there (the first one if none is flagged primary). `None` only for an
/// empty list.
pub fn monitor_at_or_primary(monitors: &[Monitor], point: PhysicalPoint) -> Option<&Monitor> {
    monitor_at(monitors, point)
        .or_else(|| monitors.iter().find(|monitor| monitor.primary))
        .or_else(|| monitors.first())
}

/// The virtual desktop: the smallest rectangle holding every monitor. Its
/// top-left is negative when a monitor sits left of or above the primary.
/// `None` for an empty list.
pub fn virtual_bounds(monitors: &[Monitor]) -> Option<PhysicalRect> {
    monitors
        .iter()
        .map(Monitor::bounds)
        .reduce(|all, bounds| all.union(&bounds))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn monitor(id: &str, bounds: (i32, i32, i32, i32), dpi: u32) -> Monitor {
        let (x, y, width, height) = bounds;
        Monitor::new(
            MonitorId::new(id),
            PhysicalRect::new(x, y, width, height),
            dpi,
            x == 0 && y == 0,
        )
    }

    /// 125% laptop panel as primary, a 100% monitor to its left and lowered,
    /// a 150% 4K monitor above.
    pub(crate) fn mixed_dpi() -> Vec<Monitor> {
        vec![
            monitor("primary", (0, 0, 1920, 1200), 120),
            monitor("left", (-1920, 200, 1920, 1080), 96),
            monitor("above", (-960, -2160, 3840, 2160), 144),
        ]
    }

    #[test]
    fn scale_follows_dpi() {
        assert_eq!(monitor("a", (0, 0, 10, 10), 96).scale(), 1.0);
        assert_eq!(monitor("a", (0, 0, 10, 10), 120).scale(), 1.25);
        assert_eq!(monitor("a", (0, 0, 10, 10), 144).scale(), 1.5);
        assert_eq!(monitor("a", (0, 0, 10, 10), 0).scale(), 1.0);
    }

    #[test]
    fn logical_size_of_a_scaled_monitor() {
        assert_eq!(
            monitor("a", (0, 0, 1920, 1200), 120).logical_size(),
            (1536, 960)
        );
        assert_eq!(
            monitor("a", (0, 0, 3840, 2160), 144).logical_size(),
            (2560, 1440)
        );
        assert_eq!(
            monitor("a", (-1920, 0, 1920, 1080), 96).logical_size(),
            (1920, 1080)
        );
    }

    #[test]
    fn finds_the_monitor_under_a_point_in_a_mixed_layout() {
        let monitors = mixed_dpi();
        let at = |x, y| monitor_at(&monitors, PhysicalPoint::new(x, y)).map(|m| m.id().as_str());
        assert_eq!(at(0, 0), Some("primary"));
        assert_eq!(at(1919, 1199), Some("primary"));
        assert_eq!(at(-1, 200), Some("left"));
        assert_eq!(at(-1920, 1279), Some("left"));
        assert_eq!(at(0, -1), Some("above"));
        assert_eq!(at(2879, -2160), Some("above"));
        // Right of the primary, and the notch between "above" and "left"
        assert_eq!(at(1920, 0), None);
        assert_eq!(at(-100, 100), None);
    }

    #[test]
    fn a_point_in_a_gap_falls_back_to_the_primary() {
        // Two monitors with a 200px gap between them
        let monitors = vec![
            monitor("far", (-2120, 0, 1920, 1080), 96),
            monitor("primary", (0, 0, 1920, 1080), 96),
        ];
        let gap = PhysicalPoint::new(-100, 500);
        assert_eq!(monitor_at(&monitors, gap), None);
        assert_eq!(
            monitor_at_or_primary(&monitors, gap).map(|m| m.id().as_str()),
            Some("primary")
        );
    }

    #[test]
    fn without_a_primary_the_first_monitor_is_the_fallback() {
        let monitors = vec![
            monitor("one", (100, 100, 800, 600), 96),
            monitor("two", (900, 100, 800, 600), 96),
        ];
        let off = PhysicalPoint::new(-5000, -5000);
        assert_eq!(
            monitor_at_or_primary(&monitors, off).map(|m| m.id().as_str()),
            Some("one")
        );
        assert_eq!(monitor_at_or_primary(&[], off), None);
    }

    #[test]
    fn virtual_bounds_start_at_the_leftmost_and_topmost_monitor() {
        assert_eq!(
            virtual_bounds(&mixed_dpi()),
            Some(PhysicalRect::new(-1920, -2160, 4800, 3440))
        );
        assert_eq!(virtual_bounds(&[]), None);
    }

    #[test]
    fn virtual_bounds_of_stacked_monitors() {
        let monitors = vec![
            monitor("bottom", (0, 0, 1920, 1080), 96),
            monitor("top", (0, -1080, 1920, 1080), 96),
        ];
        assert_eq!(
            virtual_bounds(&monitors),
            Some(PhysicalRect::new(0, -1080, 1920, 2160))
        );
    }

    #[test]
    fn virtual_bounds_include_a_gap() {
        let monitors = vec![
            monitor("primary", (0, 0, 1920, 1080), 96),
            monitor("far", (2500, 0, 1920, 1080), 96),
        ];
        assert_eq!(
            virtual_bounds(&monitors),
            Some(PhysicalRect::new(0, 0, 4420, 1080))
        );
    }

    #[test]
    fn points_round_trip_through_a_monitor_at_negative_coordinates() {
        let left = &mixed_dpi()[1];
        let desktop = PhysicalPoint::new(-1900, 250);
        let local = left.point_to_local(desktop);
        assert_eq!(local, LocalPoint::new(20, 50));
        assert_eq!(left.point_to_physical(local), desktop);
    }

    #[test]
    fn rect_to_local_clips_to_the_monitor() {
        let monitors = mixed_dpi();
        // Spans "left" and "primary"
        let window = PhysicalRect::new(-200, 300, 600, 300);
        assert_eq!(
            monitors[0].rect_to_local(window),
            Some(LocalRect::new(0, 300, 400, 300))
        );
        assert_eq!(
            monitors[1].rect_to_local(window),
            Some(LocalRect::new(1720, 100, 200, 300))
        );
        assert_eq!(monitors[2].rect_to_local(window), None);
        assert_eq!(
            monitors[1].rect_to_physical(LocalRect::new(1720, 100, 200, 300)),
            PhysicalRect::new(-200, 300, 200, 300)
        );
    }

    // The cases below are the old tst_windowsnap.cpp, which pinned the
    // physical-to-overlay mapping.

    #[test]
    fn maps_physical_to_overlay_at_one_hundred_percent() {
        let overlay = monitor("a", (0, 0, 1920, 1080), 96);
        assert_eq!(
            overlay.rect_to_logical(PhysicalRect::new(100, 50, 800, 600)),
            Some(LogicalRect::new(100, 50, 800, 600))
        );
    }

    #[test]
    fn maps_physical_to_overlay_at_one_twenty_five_percent() {
        let overlay = monitor("a", (0, 0, 1920, 1200), 120);
        assert_eq!(
            overlay.rect_to_logical(PhysicalRect::new(125, 250, 1000, 500)),
            Some(LogicalRect::new(100, 200, 800, 400))
        );
    }

    #[test]
    fn handles_a_monitor_above_the_primary() {
        let overlay = monitor("a", (973, -1080, 1920, 1080), 96);
        assert_eq!(
            overlay.rect_to_logical(PhysicalRect::new(1073, -1000, 400, 300)),
            Some(LogicalRect::new(100, 80, 400, 300))
        );
    }

    #[test]
    fn clips_a_window_spanning_two_monitors() {
        let overlay = monitor("a", (0, 0, 1920, 1080), 96);
        assert_eq!(
            overlay.rect_to_logical(PhysicalRect::new(-200, 100, 600, 300)),
            Some(LogicalRect::new(0, 100, 400, 300))
        );
    }

    #[test]
    fn drops_a_window_on_another_monitor() {
        let overlay = monitor("a", (0, 0, 1920, 1080), 96);
        assert_eq!(
            overlay.rect_to_logical(PhysicalRect::new(3000, 100, 600, 300)),
            None
        );
    }

    #[test]
    fn a_spanning_window_maps_onto_each_monitor_at_its_own_scale() {
        let monitors = mixed_dpi();
        // 400px on "left" (100%), 500px on "primary" (125%)
        let window = PhysicalRect::new(-400, 250, 900, 500);
        assert_eq!(
            monitors[1].rect_to_logical(window),
            Some(LogicalRect::new(1520, 50, 400, 500))
        );
        assert_eq!(
            monitors[0].rect_to_logical(window),
            Some(LogicalRect::new(0, 200, 400, 400))
        );
    }

    #[test]
    fn adjacent_windows_stay_adjacent_after_rounding() {
        // At 150% neither 101 nor 203 divides evenly; converting edges keeps
        // the shared edge shared
        let overlay = monitor("a", (0, 0, 3840, 2160), 144);
        let left = overlay
            .rect_to_logical(PhysicalRect::new(0, 0, 101, 100))
            .unwrap();
        let right = overlay
            .rect_to_logical(PhysicalRect::new(101, 0, 102, 100))
            .unwrap();
        assert_eq!(left.right(), right.x);
    }

    #[test]
    fn overlay_points_and_rects_map_back_to_the_desktop() {
        let above = &mixed_dpi()[2];
        assert_eq!(
            above.logical_point_to_physical(LogicalPoint::new(100, 200)),
            PhysicalPoint::new(-810, -1860)
        );
        let physical = PhysicalRect::new(-660, -1860, 1200, 600);
        let logical = above.rect_to_logical(physical).unwrap();
        assert_eq!(logical, LogicalRect::new(200, 200, 800, 400));
        assert_eq!(above.logical_rect_to_physical(logical), physical);
    }
}
