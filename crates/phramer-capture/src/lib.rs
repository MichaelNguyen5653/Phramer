//! Phramer capture: monitors, frozen screenshots and the window list for
//! window snap.
//!
//! # Coordinate spaces
//!
//! Three spaces are in play and each has its own types, so they cannot be
//! mixed by accident:
//!
//! - **Physical** ([`PhysicalPoint`], [`PhysicalRect`]): physical pixels in
//!   virtual-desktop coordinates. This is what Win32 reports to a
//!   per-monitor-DPI-aware process, what DWM reports for window frames and
//!   what a grab contains. Monitors left of or above the primary sit at
//!   **negative** coordinates; never assume the desktop starts at (0,0).
//! - **Local** ([`LocalPoint`], [`LocalRect`]): physical pixels relative to
//!   one monitor's top-left corner, i.e. indices into that monitor's grab.
//! - **Logical** ([`LogicalPoint`], [`LogicalRect`]): device-independent
//!   pixels relative to one monitor's top-left corner, i.e. what an overlay
//!   window covering that monitor lays out in. Physical = logical x scale.
//!
//! There is deliberately no logical space for the whole desktop: every
//! monitor has its own scale factor, so no single scale maps a logical union
//! of monitors onto physical space. That is why the overlay is one window
//! per monitor and why every mapping here goes through one [`Monitor`].
//!
//! # DPI awareness
//!
//! Everything above holds only in a per-monitor-DPI-aware (v2) process. The
//! binary must call [`enable_per_monitor_dpi_awareness`] before it creates a
//! window or calls anything else in this crate. The live functions refuse to
//! run otherwise rather than return virtualised coordinates.

mod error;
mod frame;
mod geometry;
mod monitor;
mod region;
mod snap;
#[cfg(windows)]
mod win;

pub use error::CaptureError;
pub use frame::Frame;
pub use geometry::{LocalPoint, LocalRect, LogicalPoint, LogicalRect, PhysicalPoint, PhysicalRect};
pub use monitor::{Monitor, MonitorId, monitor_at, monitor_at_or_primary, virtual_bounds};
pub use region::{CaptureTarget, RegionMode, choose_target};
pub use snap::window_at;
#[cfg(windows)]
pub use win::{
    cursor_position, enable_per_monitor_dpi_awareness, enumerate_monitors, grab_monitor,
    grab_virtual_desktop, visible_windows,
};
