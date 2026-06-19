//! Event handling for the TUI.

use crossterm::event::{self, Event};
use std::time::Duration;

/// Poll for terminal events with a timeout.
pub fn poll_event(timeout: Duration) -> std::io::Result<Option<Event>> {
    if event::poll(timeout)? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}
