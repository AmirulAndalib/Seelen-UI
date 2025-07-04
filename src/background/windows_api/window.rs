use seelen_core::{rect::Rect, state::AppExtraFlag};
use std::fmt::{Debug, Display};

use windows::{
    ApplicationModel::AppInfo,
    Win32::{
        Foundation::{HWND, RECT},
        UI::WindowsAndMessaging::{
            SET_WINDOW_POS_FLAGS, SHOW_WINDOW_CMD, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW,
        },
    },
};

use crate::{
    cli::{ServicePipe, SvcAction},
    error_handler::Result,
    modules::{
        start::application::START_MENU_MANAGER,
        virtual_desk::{get_vd_manager, VirtualDesktop},
    },
    seelen_bar::FancyToolbar,
    seelen_rofi::SeelenRofi,
    seelen_wall::SeelenWall,
    seelen_weg::instance::SeelenWeg,
    seelen_wm_v2::instance::WindowManagerV2,
    state::application::FULL_STATE,
};

use super::{
    monitor::Monitor, process::Process, types::AppUserModelId, WindowEnumerator, WindowsApi,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Window(HWND);
unsafe impl Send for Window {}
unsafe impl Sync for Window {}

impl From<HWND> for Window {
    fn from(hwnd: HWND) -> Self {
        Self(hwnd)
    }
}

impl From<isize> for Window {
    fn from(addr: isize) -> Self {
        Self(HWND(addr as _))
    }
}

impl Debug for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Window")
            .field("handle", &self.0 .0)
            .field(
                "exe",
                &self.process().program_exe_name().unwrap_or_default(),
            )
            .field("class", &self.class())
            .field("title", &self.title())
            .finish()
    }
}

impl Display for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Window({:?})", self.0)
    }
}

pub const APP_FRAME_HOST_PATH: &str = "C:\\Windows\\System32\\ApplicationFrameHost.exe";
impl Window {
    pub fn get_foregrounded() -> Window {
        Window(WindowsApi::get_foreground_window())
    }

    pub fn hwnd(&self) -> HWND {
        self.0
    }

    pub fn address(&self) -> isize {
        self.0 .0 as isize
    }

    pub fn is_electron(&self) -> bool {
        self.class() == "Chrome_WidgetWin_1"
    }

    /// Application user model id asigned to the window via property-store or inherited from the process
    ///
    /// https://learn.microsoft.com/en-us/windows/win32/properties/props-system-appusermodel-id
    pub fn app_user_model_id(&self) -> Option<AppUserModelId> {
        if let Ok(umid) = WindowsApi::get_window_app_user_model_id(self.0) {
            return match WindowsApi::is_uwp_package_id(&umid) {
                true => Some(AppUserModelId::Appx(umid)),
                false => Some(AppUserModelId::PropertyStore(umid)),
            };
        }

        let process = self.process();
        if let Ok(umid) = process.package_app_user_model_id() {
            return Some(umid);
        }

        if self.is_electron() {
            let path = process.program_path().ok()?;

            // special manual case like there's no way to call GetCurrentProcessExplicitAppUserModelID without code injection
            if path.file_name()?.to_string_lossy().to_lowercase() == "discord.exe" {
                return Some(AppUserModelId::PropertyStore(
                    "com.squirrel.Discord.Discord".to_string(),
                ));
            }

            let guard = START_MENU_MANAGER.load();
            let item = guard.get_by_target(&path)?;
            Some(AppUserModelId::PropertyStore(item.umid.clone()?))
        } else {
            None
        }
    }

    /// https://learn.microsoft.com/en-us/windows/win32/properties/props-system-appusermodel-preventpinning
    pub fn prevent_pinning(&self) -> bool {
        WindowsApi::get_window_prevent_pinning(self.0).unwrap_or(false)
    }

    /// https://learn.microsoft.com/en-us/windows/win32/properties/props-system-appusermodel-relaunchcommand
    pub fn relaunch_command(&self) -> Option<String> {
        WindowsApi::get_window_relaunch_command(self.0).ok()
    }

    /// https://learn.microsoft.com/en-us/windows/win32/properties/props-system-appusermodel-relaunchdisplaynameresource
    pub fn relaunch_display_name(&self) -> Option<String> {
        if let Ok(name) = WindowsApi::get_window_relaunch_display_name(self.0) {
            if name.starts_with("@") {
                return WindowsApi::resolve_indirect_string(&name).ok();
            }
            return Some(name);
        }
        None
    }

    /// https://learn.microsoft.com/en-us/windows/win32/properties/props-system-appusermodel-relaunchiconresource
    #[allow(dead_code)]
    pub fn relaunch_icon(&self) -> Option<String> {
        WindowsApi::get_window_relaunch_icon_resource(self.0).ok()
    }

    pub fn title(&self) -> String {
        WindowsApi::get_window_text(self.0)
    }

    pub fn class(&self) -> String {
        WindowsApi::get_class(self.0).unwrap_or_default()
    }

    pub fn process(&self) -> Process {
        Process::from_window(self)
    }

    pub fn app_display_name(&self) -> Result<String> {
        if let Some(AppUserModelId::Appx(umid)) = self.app_user_model_id() {
            let info = AppInfo::GetFromAppUserModelId(&umid.into())?;
            return Ok(info.DisplayInfo()?.DisplayName()?.to_string_lossy());
        }
        self.process().program_display_name()
    }

    #[allow(dead_code)]
    pub fn outer_rect(&self) -> Result<Rect> {
        let rect = WindowsApi::get_outer_window_rect(self.hwnd())?;
        Ok(Rect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        })
    }

    pub fn inner_rect(&self) -> Result<Rect> {
        let rect = WindowsApi::get_inner_window_rect(self.hwnd())?;
        Ok(Rect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        })
    }

    pub fn parent(&self) -> Option<Window> {
        match WindowsApi::get_parent(self.0) {
            Ok(parent) => Some(Window::from(parent)),
            Err(_) => None,
        }
    }

    pub fn children(&self) -> Result<Vec<Window>> {
        WindowEnumerator::new()
            .with_parent(self.0)
            .map(Window::from)
    }

    pub fn monitor(&self) -> Monitor {
        Monitor::from(WindowsApi::monitor_from_window(self.0))
    }

    pub fn workspace(&self) -> Result<VirtualDesktop> {
        get_vd_manager().get_by_window(self.address())
    }

    pub fn is_window(&self) -> bool {
        WindowsApi::is_window(self.0)
    }

    pub fn is_visible(&self) -> bool {
        WindowsApi::is_window_visible(self.0)
    }

    pub fn is_minimized(&self) -> bool {
        WindowsApi::is_iconic(self.0)
    }

    pub fn is_maximized(&self) -> bool {
        WindowsApi::is_zoomed(self.0)
    }

    pub fn is_cloaked(&self) -> bool {
        WindowsApi::is_cloaked(self.0).unwrap_or(false)
    }

    pub fn is_focused(&self) -> bool {
        WindowsApi::get_foreground_window() == self.0
    }

    pub fn is_fullscreen(&self) -> bool {
        WindowsApi::is_fullscreen(self.0).unwrap_or(false)
    }

    /// is the window an Application Frame Host
    pub fn is_frame(&self) -> Result<bool> {
        Ok(self
            .process()
            .program_path()?
            .as_os_str()
            .eq_ignore_ascii_case(APP_FRAME_HOST_PATH))
    }

    /// will fail if the window is not a frame
    pub fn get_frame_creator(&self) -> Result<Option<Window>> {
        if !self.is_frame()? {
            return Err("Window is not a frame".into());
        }
        for window in self.children()? {
            if !window.class().starts_with("ApplicationFrame") {
                return Ok(Some(window));
            }
        }
        Ok(None)
    }

    /// this means all windows that are part of the UI desktop not the real desktop window
    pub fn is_desktop(&self) -> bool {
        WindowsApi::get_desktop_window() == self.0 || {
            let class = self.class();
            class == "Progman" || {
                class == "WorkerW"
                    && self.children().is_ok_and(|children| {
                        children
                            .iter()
                            .any(|child| child.class() == "SHELLDLL_DefView")
                    })
            }
        }
    }

    pub fn is_seelen_overlay(&self) -> bool {
        if let Ok(exe) = self.process().program_path() {
            return exe.ends_with("seelen-ui.exe")
                && [
                    FancyToolbar::TITLE,
                    WindowManagerV2::TITLE,
                    SeelenWeg::TITLE,
                    SeelenRofi::TITLE,
                    SeelenWall::TITLE,
                ]
                .contains(&self.title().as_str());
        }
        false
    }

    pub fn is_real_window(&self) -> bool {
        // unmanageable window
        if self.process().open_limited_handle().is_err() {
            return false;
        }

        if !self.is_visible() || self.parent().is_some() {
            return false;
        }

        // ignore windows without a title or that start with a dot
        // this is a seelen ui behavior, not present on other desktop environments
        let title = self.title();
        if title.is_empty() || title.starts_with('.') {
            return false;
        }

        // this class is used for edge tabs to be shown as independent windows on alt + tab
        // this only applies when the new tab is created it is binded to explorer.exe for some reason
        // maybe we can search/learn more about edge tabs later.
        // fix: https://github.com/eythaann/Seelen-UI/issues/83
        if self.class() == "Windows.Internal.Shell.TabProxyWindow" {
            return false;
        }

        let ex_style = WindowsApi::get_ex_styles(self.hwnd());
        if (ex_style.contains(WS_EX_TOOLWINDOW) || ex_style.contains(WS_EX_NOACTIVATE))
            && !ex_style.contains(WS_EX_APPWINDOW)
        {
            return false;
        }

        if let Ok(frame_creator) = self.get_frame_creator() {
            if frame_creator.is_none() {
                return false;
            }
        }

        if self.process().is_frozen().unwrap_or(false) {
            return false;
        }

        // I don't like to determine if a window is real filtering by this configs, but will be here
        // as a workaround in meantime we find a way to filter better, as native taskbar does.
        if let Some(config) = FULL_STATE.load().get_app_config_by_window(self.hwnd()) {
            if config.options.contains(&AppExtraFlag::Hidden) {
                return false;
            }
        }

        true
    }

    pub fn show_window(&self, command: SHOW_WINDOW_CMD) -> Result<()> {
        if self.process().open_handle().is_ok() {
            WindowsApi::show_window(self.hwnd(), command)
        } else {
            ServicePipe::request(SvcAction::ShowWindow {
                hwnd: self.address(),
                command: command.0,
            })
        }
    }

    pub fn show_window_async(&self, command: SHOW_WINDOW_CMD) -> Result<()> {
        if self.process().open_handle().is_ok() {
            WindowsApi::show_window_async(self.hwnd(), command)
        } else {
            ServicePipe::request(SvcAction::ShowWindowAsync {
                hwnd: self.address(),
                command: command.0,
            })
        }
    }

    pub fn set_position(&self, rect: &RECT, flags: SET_WINDOW_POS_FLAGS) -> Result<()> {
        if self.process().open_handle().is_ok() {
            WindowsApi::set_position(self.hwnd(), None, rect, flags)
        } else {
            ServicePipe::request(SvcAction::SetWindowPosition {
                hwnd: self.address(),
                x: rect.left,
                y: rect.top,
                width: (rect.right - rect.left).abs(),
                height: (rect.bottom - rect.top).abs(),
                flags: flags.0,
            })
        }
    }

    pub fn focus(&self) -> Result<()> {
        if self.process().open_handle().is_ok() {
            WindowsApi::set_foreground(self.hwnd())
        } else {
            ServicePipe::request(SvcAction::SetForeground(self.address()))
        }
    }
}
