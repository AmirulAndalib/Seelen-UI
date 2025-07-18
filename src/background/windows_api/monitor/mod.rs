mod brightness;

use brightness::DisplayDevice;
use itertools::Itertools;
use windows::Win32::Graphics::Gdi::HMONITOR;

use crate::{error_handler::Result, modules::input::domain::Point};
use seelen_core::rect::Rect;

use super::{MonitorEnumerator, WindowsApi};

/// This struct represents a screen, a screen could be shown in multiple display devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor(HMONITOR);

unsafe impl Send for Monitor {}
unsafe impl Sync for Monitor {}

impl From<HMONITOR> for Monitor {
    fn from(hmonitor: HMONITOR) -> Self {
        Self(hmonitor)
    }
}

impl From<isize> for Monitor {
    fn from(hmonitor: isize) -> Self {
        Self(HMONITOR(hmonitor as _))
    }
}

impl From<&Point> for Monitor {
    fn from(point: &Point) -> Self {
        let hmonitor = WindowsApi::monitor_from_point(point);
        Self(hmonitor)
    }
}

// HMONITOR on win32 is the same concept as DisplayView in winrt

impl Monitor {
    pub fn handle(&self) -> HMONITOR {
        self.0
    }

    pub fn address(&self) -> usize {
        self.0 .0 as _
    }

    pub fn at(index: usize) -> Option<Monitor> {
        let monitors = MonitorEnumerator::get_all_v2().ok()?;
        monitors.get(index).copied()
    }

    pub fn index(&self) -> Result<usize> {
        let monitors = MonitorEnumerator::get_all_v2()?;
        let (idx, _) = monitors
            .into_iter()
            .find_position(|monitor| monitor == self)
            .ok_or("Invalid or expired monitor handle")?;
        Ok(idx)
    }

    pub fn by_id(id: &str) -> Option<Monitor> {
        for m in MonitorEnumerator::get_all_v2().ok()? {
            if let Ok(monitor_device_id) = m.stable_id() {
                if monitor_device_id == id {
                    return Some(m);
                }
            }
        }
        None
    }

    pub fn primary() -> Monitor {
        Monitor(WindowsApi::primary_monitor())
    }

    pub fn is_primary(&self) -> bool {
        self.0 == WindowsApi::primary_monitor()
    }

    pub fn name(&self) -> Result<String> {
        let target = WindowsApi::get_monitor_target_by_idx(self.index()?)?;
        Ok(target.TryGetMonitor()?.DisplayName()?.to_string())
    }

    pub fn stable_id(&self) -> Result<String> {
        let target = WindowsApi::get_monitor_target_by_idx(self.index()?)?;
        Ok(target.StableMonitorId()?.to_string())
    }

    fn diplay_devices(&self) -> Result<Vec<DisplayDevice>> {
        WindowsApi::get_display_devices(self.0)
            .map(|list| list.iter().map(DisplayDevice::from).collect())
    }

    /// the first display device is the primary
    pub fn main_display_device(&self) -> Result<DisplayDevice> {
        let diplay_devices = self.diplay_devices()?;
        for device in diplay_devices {
            if device.is_enabled()? {
                return Ok(device);
            }
        }
        Err("No enabled display device for this monitor".into())
    }

    pub fn rect(&self) -> Result<Rect> {
        let rect = WindowsApi::monitor_info(self.0)?.monitorInfo.rcMonitor;
        Ok(Rect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        })
    }

    #[allow(dead_code)]
    pub fn scale_factor(&self) -> Result<f64> {
        let monitor_scale_factor = WindowsApi::get_monitor_scale_factor(self.0)?;
        let text_scale_factor = WindowsApi::get_text_scale_factor()?;
        Ok(monitor_scale_factor * text_scale_factor)
    }
}
