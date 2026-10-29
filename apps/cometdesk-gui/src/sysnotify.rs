//! Desktop notifications for messages from the target while the window is in the background.

/// Shows a system notification; `false` where this platform has no way to.
#[cfg(target_os = "macos")]
pub fn show(title: &str, body: &str, subtitle: &str) -> bool {
    // The texts come from the target, so they go in as arguments, never into the script.
    std::process::Command::new("osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 2 of argv) with title (item 1 of argv) subtitle (item 3 of argv)",
            "-e",
            "end run",
            "--",
            title,
            body,
            subtitle,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|mut child| {
            // Reap it without blocking the UI.
            std::thread::spawn(move || child.wait());
        })
        .is_ok()
}

#[cfg(not(target_os = "macos"))]
pub fn show(_: &str, _: &str, _: &str) -> bool {
    false
}
