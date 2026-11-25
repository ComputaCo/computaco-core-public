//! Visual Studio Code's frame without the editor: the title bar and the command and
//! menu tables the platform shells draw a Code window's frame and menus from. The editor
//! itself is a registered application of its own crate (`apps::external`), and draws its
//! frame with these too.
pub mod commands;
pub mod frame;
pub mod icons;
pub use frame::title_bar;
