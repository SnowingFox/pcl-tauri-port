//! Local instance (version) management: scanning `.minecraft` folders,
//! detecting loader / MC version, categories and icons, and file operations.
//!
//! Ported from PCL `ModMinecraft.vb` (`McInstance`, `McInstanceCardType`) and
//! PCL.Mac `MinecraftInstance`. Derived from Plain Craft Launcher 2.

pub mod commands;
pub mod detect;
pub mod ops;
pub mod scan;

