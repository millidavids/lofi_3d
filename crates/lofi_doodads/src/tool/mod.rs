//! Placing, picking, moving and changing objects.
//!
//! Every change is undoable. A drag is one recorded edit, so undo is refused
//! until it's let go; single actions (place, rotate, delete) are one step
//! each.

mod edit;
mod mouse;
mod settings;

#[cfg(test)]
mod tests;

pub(crate) use edit::{Edit, apply_edit, edit_objects, forget_vanished, object_keys};
pub use mouse::ObjectDrag;
pub(crate) use mouse::run_mouse;
pub use settings::{DoodadSettings, DoodadTool, SelectedObject};
