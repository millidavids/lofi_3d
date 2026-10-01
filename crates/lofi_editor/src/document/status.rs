//! The one-line message in the status bar.

use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct StatusLine {
    text: String,
    is_error: bool,
}

impl StatusLine {
    pub fn info(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.is_error = false;
        info!("{}", self.text);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.is_error = true;
        error!("{}", self.text);
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn is_error(&self) -> bool {
        self.is_error
    }
}
