//! Touch ID gate in front of keychain reads (LocalAuthentication; macOS password as fallback).
//!
//! An app-level check: the keychain item itself is still guarded by its own ACL. The OS-enforced
//! alternative (a Data Protection item requiring user presence) needs a provisioning profile.

/// Blocks until the user authenticates. `Ok` too when the Mac has neither Touch ID nor a
/// password to ask for, so an unlockable setup never locks the user out of their devices.
#[cfg(target_os = "macos")]
pub fn authenticate(reason: &str) -> Result<(), String> {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};

    let policy = LAPolicy::DeviceOwnerAuthentication;
    let context = unsafe { LAContext::new() };
    if unsafe { context.canEvaluatePolicy_error(policy) }.is_err() {
        return Ok(());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let reply = RcBlock::new(move |ok: Bool, err: *mut NSError| {
        let result = if ok.as_bool() {
            Ok(())
        } else {
            Err(unsafe { err.as_ref() }
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "authentication failed".into()))
        };
        let _ = tx.send(result);
    });
    unsafe {
        context.evaluatePolicy_localizedReason_reply(policy, &NSString::from_str(reason), &reply)
    };
    rx.recv().map_err(|e| e.to_string())?
}

#[cfg(not(target_os = "macos"))]
pub fn authenticate(_reason: &str) -> Result<(), String> {
    Ok(())
}
