//! Global settings store.
//!
//! Ported in spirit from PCL's `Settings.vb` (key/value entries in registry / ini)
//! and PCL.Mac's `AppSettings`. On macOS the settings live in a single JSON file
//! in the app data dir, written atomically, with schema versioning + migration.
//!
//! Derived from Plain Craft Launcher 2 (see LICENCE in repo root).

pub mod commands;
pub mod guard;
pub mod instance_settings;
pub mod model;
pub mod store;

pub use instance_settings::InstanceSettings;
pub use model::*;
