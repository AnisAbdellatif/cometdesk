//! The target's master output volume and mute: read for `Info`, set on request.

use cometdesk_clip::Volume;

pub fn read() -> Option<Volume> {
    sys::read()
}

/// Sets the level (0-100) and/or the mute state.
pub fn set(level: Option<u8>, muted: Option<bool>) -> Result<(), String> {
    sys::set(level.map(|l| l.min(100)), muted)
}

// ----- Linux: PulseAudio / PipeWire (`pactl`), then WirePlumber (`wpctl`), then ALSA

#[cfg(all(unix, not(target_os = "macos")))]
mod sys {
    use std::process::Command;

    use super::*;

    fn out(cmd: &str, args: &[&str]) -> Option<String> {
        let o = Command::new(cmd).args(args).output().ok()?;
        o.status
            .success()
            .then(|| String::from_utf8_lossy(&o.stdout).into_owned())
    }

    fn exec(cmd: &str, args: &[&str]) -> Option<()> {
        Command::new(cmd)
            .args(args)
            .status()
            .ok()
            .filter(|s| s.success())
            .map(|_| ())
    }

    /// The first `NN%` in the output.
    fn percent(text: &str) -> Option<u8> {
        let end = text.find('%')?;
        let start = text[..end]
            .rfind(|c: char| !c.is_ascii_digit())
            .map_or(0, |i| i + 1);
        text[start..end]
            .parse::<u32>()
            .ok()
            .map(|p| p.min(100) as u8)
    }

    pub fn read() -> Option<Volume> {
        if let (Some(v), Some(m)) = (
            out("pactl", &["get-sink-volume", "@DEFAULT_SINK@"]),
            out("pactl", &["get-sink-mute", "@DEFAULT_SINK@"]),
        ) {
            return Some(Volume {
                percent: percent(&v)?,
                muted: m.contains("yes"),
            });
        }
        if let Some(v) = out("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]) {
            // `Volume: 0.40` or `Volume: 0.40 [MUTED]`
            let level: f32 = v.split_whitespace().nth(1)?.parse().ok()?;
            return Some(Volume {
                percent: (level * 100.0).round().clamp(0.0, 100.0) as u8,
                muted: v.contains("MUTED"),
            });
        }
        // `  Front Left: 41943 [64%] [on]`
        let v = out("amixer", &["get", "Master"])?;
        Some(Volume {
            percent: percent(&v)?,
            muted: v.contains("[off]"),
        })
    }

    pub fn set(level: Option<u8>, muted: Option<bool>) -> Result<(), String> {
        let mut done = false;
        if let Some(l) = level {
            done = exec(
                "pactl",
                &["set-sink-volume", "@DEFAULT_SINK@", &format!("{l}%")],
            )
            .or_else(|| {
                let v = format!("{:.2}", f32::from(l) / 100.0);
                exec("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &v])
            })
            .or_else(|| exec("amixer", &["sset", "Master", &format!("{l}%")]))
            .is_some();
        }
        if let Some(m) = muted {
            let flag = if m { "1" } else { "0" };
            done = exec("pactl", &["set-sink-mute", "@DEFAULT_SINK@", flag])
                .or_else(|| exec("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", flag]))
                .or_else(|| {
                    exec(
                        "amixer",
                        &["sset", "Master", if m { "mute" } else { "unmute" }],
                    )
                })
                .is_some();
        }
        if done {
            Ok(())
        } else {
            Err("no sound server answered (pactl, wpctl, amixer)".into())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn percent_is_found_in_each_tool_output() {
            let pa = "Volume: front-left: 41943 /  64% / -11.96 dB,   front-right: 41943 /  64%";
            assert_eq!(percent(pa), Some(64));
            assert_eq!(percent("  Front Left: 41943 [100%] [on]"), Some(100));
            assert_eq!(percent("no volume"), None);
        }
    }
}

// ----- macOS

#[cfg(target_os = "macos")]
mod sys {
    use std::process::Command;

    use super::*;

    fn osa(script: &str) -> Result<String, String> {
        let o = Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| e.to_string())?;
        if o.status.success() {
            Ok(String::from_utf8_lossy(&o.stdout).trim().to_owned())
        } else {
            Err(String::from_utf8_lossy(&o.stderr).trim().to_owned())
        }
    }

    pub fn read() -> Option<Volume> {
        let s = osa("get volume settings").ok()?;
        // `output volume:50, input volume:75, alert volume:100, output muted:false`
        let field = |name: &str| s.split(name).nth(1)?.split(',').next().map(str::trim);
        Some(Volume {
            percent: field("output volume:")?.parse().ok()?,
            muted: field("output muted:")? == "true",
        })
    }

    pub fn set(level: Option<u8>, muted: Option<bool>) -> Result<(), String> {
        if let Some(l) = level {
            osa(&format!("set volume output volume {l}"))?;
        }
        if let Some(m) = muted {
            osa(&format!("set volume output muted {m}"))?;
        }
        Ok(())
    }
}

// ----- Windows: the default render endpoint's IAudioEndpointVolume

#[cfg(windows)]
mod sys {
    use std::cell::Cell;

    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender,
    };
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    };

    use super::*;

    thread_local! {
        static COM: Cell<bool> = const { Cell::new(false) };
    }

    fn endpoint() -> windows::core::Result<IAudioEndpointVolume> {
        if !COM.replace(true) {
            // Fails harmlessly when the thread already joined an apartment.
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        }
        unsafe {
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
            device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
        }
    }

    pub fn read() -> Option<Volume> {
        let ep = endpoint().ok()?;
        unsafe {
            Some(Volume {
                percent: (ep.GetMasterVolumeLevelScalar().ok()? * 100.0)
                    .round()
                    .clamp(0.0, 100.0) as u8,
                muted: ep.GetMute().ok()?.as_bool(),
            })
        }
    }

    pub fn set(level: Option<u8>, muted: Option<bool>) -> Result<(), String> {
        let ep = endpoint().map_err(|e| format!("no audio endpoint: {e}"))?;
        unsafe {
            if let Some(l) = level {
                ep.SetMasterVolumeLevelScalar(f32::from(l) / 100.0, std::ptr::null())
                    .map_err(|e| e.to_string())?;
            }
            if let Some(m) = muted {
                ep.SetMute(m, std::ptr::null()).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}
