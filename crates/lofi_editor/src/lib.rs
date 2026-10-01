//! The editor shell every lofi tool shares: the map document with undo,
//! files and autosave; the fly camera; the terrain, water and sky view; the
//! menu and status bars and prompts; picking; screenshots and measuring.
//!
//! A tool builds on [`app::editor_app`], adds its own plugins and side panel
//! (in [`ui::UiSystems::Panels`]), and runs.

pub mod app;
pub mod camera;
mod capture;
pub mod document;
pub mod launch;
pub mod pick;
pub mod schedule;
pub mod ui;
pub mod view;
