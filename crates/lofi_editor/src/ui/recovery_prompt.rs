//! "Recover unsaved work?" after a crash left an autosave behind.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::document::{PendingRecovery, RecoveryMessage};

pub(super) fn recovery_prompt(
    mut contexts: EguiContexts,
    pending: Res<PendingRecovery>,
    mut choice: MessageWriter<RecoveryMessage>,
) -> Result {
    let Some(explanation) = pending.explanation() else {
        return Ok(());
    };
    let ctx = contexts.ctx_mut()?;
    egui::Modal::new(egui::Id::new("recover_autosave")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.heading("Recover unsaved work?");
        ui.label(explanation);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Recover").clicked() {
                choice.write(RecoveryMessage::Recover);
            }
            if ui.button("Discard").clicked() {
                choice.write(RecoveryMessage::Discard);
            }
        });
    });
    Ok(())
}
