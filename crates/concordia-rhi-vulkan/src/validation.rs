//! Collects the Khronos validation layer's error messages through a debug-utils messenger.

use std::ffi::c_void;
use std::sync::{Arc, Mutex};

use ash::vk;

/// Error-severity messages the validation layer reported; tests fail when it is not empty.
#[derive(Debug, Clone, Default)]
pub struct ValidationLog(Arc<Mutex<Vec<String>>>);

impl ValidationLog {
    /// An empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Every error reported so far.
    pub fn errors(&self) -> Vec<String> {
        self.0.lock().map(|e| e.clone()).unwrap_or_default()
    }

    /// Removes and returns every error reported so far.
    pub fn take(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|mut e| std::mem::take(&mut *e))
            .unwrap_or_default()
    }

    fn push(&self, message: String) {
        if let Ok(mut errors) = self.0.lock() {
            errors.push(message);
        }
    }

    /// The pointer the messenger hands back to [`callback`]; valid while this log lives.
    pub(crate) fn user_data(&self) -> *mut c_void {
        Arc::as_ptr(&self.0).cast_mut().cast()
    }
}

/// The messenger: records error-severity messages and never aborts the call.
///
/// # Safety
/// Vulkan calls it with `user_data` from [`ValidationLog::user_data`] of a log that outlives the
/// messenger, and a valid `data` pointer.
pub(crate) unsafe extern "system" fn callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    user_data: *mut c_void,
) -> vk::Bool32 {
    if !severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) || user_data.is_null() {
        return vk::FALSE;
    }
    // SAFETY: Vulkan passes a valid callback-data pointer (or null, checked) for the call's duration.
    let message = unsafe { data.as_ref() }
        // SAFETY: `p_message` is null or a nul-terminated string owned by the caller.
        .and_then(|d| unsafe { d.message_as_c_str() })
        .map_or_else(
            || "(no message)".to_owned(),
            |m| m.to_string_lossy().into_owned(),
        );
    // SAFETY: `user_data` points at the `Mutex` inside a live `ValidationLog`'s `Arc` (see above);
    // the clone keeps its count balanced while the caller's handle owns it.
    let log = unsafe {
        let inner = user_data.cast::<Mutex<Vec<String>>>().cast_const();
        Arc::increment_strong_count(inner);
        ValidationLog(Arc::from_raw(inner))
    };
    log.push(message);
    vk::FALSE
}
