//! Quality of service configuration for latency-critical worker threads.

/// Sets macOS `QOS_CLASS_USER_INTERACTIVE` on the calling thread so the scheduler keeps it on performance cores.
#[cfg(target_os = "macos")]
pub fn set_user_interactive_qos() {
    // SAFETY: pthread_set_qos_class_self_np operates only on the calling thread and does not mutate memory.
    let ret = unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0)
    };
    if ret != 0 {
        tracing::debug!(
            ret,
            "failed to set QOS_CLASS_USER_INTERACTIVE on current thread"
        );
    }
}

/// Sets macOS `QOS_CLASS_USER_INTERACTIVE` on the calling thread so the scheduler keeps it on performance cores.
#[cfg(not(target_os = "macos"))]
pub fn set_user_interactive_qos() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_qos_without_panicking() {
        set_user_interactive_qos();
    }
}
