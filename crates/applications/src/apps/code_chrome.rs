//! Visual Studio Code's frame without the editor: the title bar and the command and
//! menu tables the platform shells draw a Code window's frame and menus from. A build
//! that serves the editor from its module (`apps::module`) keeps these, since the
//! frame is the shell's to draw.
#[path = "code/commands.rs"]
pub mod commands;
#[path = "code/frame.rs"]
pub mod frame;
#[path = "code/icons.rs"]
mod icons;
pub use frame::title_bar;
