//! Desktop-only agent plugins: embedded Browser tools + OS Computer Use.
//!
//! These plugins hold a Tauri [`AppHandle`] and talk to the shell directly.
//! They are not portable to the headless runner — register them only from the
//! desktop composition root.

mod browser;
mod computer;
mod cursor_overlay;

pub use browser::BrowserPlugin;
pub use computer::ComputerPlugin;
