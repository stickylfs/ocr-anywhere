pub mod monitor;
pub mod screen;

pub use monitor::{get_all_monitors, get_cursor_position, get_target_monitor, MonitorInfo};
pub use screen::{
    capture_monitor_snapshot, create_software_bitmap, crop_region, CapturedFrame, CapturedRegion,
};
