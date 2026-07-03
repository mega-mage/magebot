use ratatui::style::Color;

pub const STATUS_BAR_BG: Color = Color::Rgb(30, 30, 46);
pub const TAB_ACTIVE_FG: Color = Color::Rgb(137, 180, 250);
pub const TAB_ACTIVE_BG: Color = Color::Rgb(49, 50, 68);
pub const TAB_INACTIVE_FG: Color = Color::Rgb(166, 173, 200);

pub const BUTTON_FG: Color = Color::Rgb(205, 214, 244);
#[allow(dead_code)]
pub const BUTTON_BG: Color = Color::Rgb(49, 50, 68);
#[allow(dead_code)]
pub const BUTTON_HIGHLIGHT: Color = Color::Rgb(137, 180, 250);
#[allow(dead_code)]
pub const BUTTON_DISABLED: Color = Color::Rgb(108, 112, 134);

pub const BORDER_FOCUSED: Color = Color::Cyan;
pub const BORDER_NORMAL: Color = Color::DarkGray;

pub const LOG_INFO: Color = Color::LightGreen;
pub const LOG_WARN: Color = Color::LightYellow;
pub const LOG_ERROR: Color = Color::Red;

pub const CONNECTED: Color = Color::Green;
pub const DISCONNECTED: Color = Color::Red;
pub const CONNECTING: Color = Color::Yellow;

pub const PROGRESS_BAR_FG: Color = Color::Blue;
pub const PROGRESS_BAR_BG: Color = Color::DarkGray;
pub const PROGRESS_COMPLETE: Color = Color::Green;

pub const TOGGLE_ON: Color = Color::LightCyan;
pub const TOGGLE_OFF: Color = Color::DarkGray;

pub const NOTIFY_SUCCESS: Color = Color::Green;
pub const NOTIFY_WARNING: Color = Color::Yellow;
pub const NOTIFY_ERROR: Color = Color::Red;
pub const NOTIFY_INFO: Color = Color::Cyan;
