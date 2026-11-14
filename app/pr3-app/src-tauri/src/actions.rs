//! No consequential connector is installed in the document-search app.
//! Old waiting calls fail closed; the trusted gate remains tested in pr3-core.
use pr3_core::trusted::{ApprovalRequest, Contact, Mail};
use pr3_core::{ToolCall, ToolSpec};
use std::sync::{Arc, atomic::AtomicBool};
pub(crate) const AUDIT_FAILED: &str =
    "The action audit could not be saved. Further actions are disabled until PR3 restarts.";
pub struct Actions;
impl Actions {
    pub fn tools(&self) -> Vec<ToolSpec> {
        Vec::new()
    }
    pub fn answer(&self, _: &str, _: bool) -> Result<(), String> {
        Err("This action connector has been removed.".into())
    }
    pub(crate) async fn run_checked(
        self: Arc<Self>,
        _: &ToolCall,
        _: impl FnOnce(&Mail) -> Result<(), String>,
        _: impl FnOnce(&ApprovalRequest),
        _: Arc<AtomicBool>,
        _: impl FnOnce() -> Result<(), String> + Send + 'static,
    ) -> (bool, String) {
        (
            false,
            "This action connector has been removed; nothing was executed.".into(),
        )
    }
}
pub(crate) fn names(contacts: &[Contact]) -> String {
    contacts
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
