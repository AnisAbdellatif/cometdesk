//! Command-line argument and subcommand definitions (`clap` derive types).
//!
//! Entry points: `Cli`, `Cmd`, `MsdCmd`, `BridgeCmd`, and value-enum argument types.
//! To add or modify CLI flags or help text, edit this file.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(version, about)]
pub(crate) struct Cli {
    /// Device address (host or host:port). Defaults to $COMETDESK_HOST.
    #[arg(long, global = true)]
    pub(crate) host: Option<String>,
    /// User name. Defaults to $COMETDESK_USER, then "admin".
    #[arg(long, global = true)]
    pub(crate) user: Option<String>,
    /// Current 2FA (TOTP) code. Defaults to $COMETDESK_TOTP; asked for when needed.
    #[arg(long, global = true)]
    pub(crate) totp: Option<String>,
    #[command(subcommand)]
    pub(crate) cmd: Cmd,
}

#[derive(Subcommand)]
pub(crate) enum Cmd {
    /// Open a console window: video + keyboard and mouse.
    View {
        /// Send Cmd as Ctrl and Ctrl as Cmd (for Windows/Linux targets).
        #[arg(long)]
        swap_cmd_ctrl: bool,
        /// Fix the swapped ISO keys (§/^ and </>) that macOS reports on ISO keyboards.
        #[arg(long, value_enum, default_value_t = Toggle::Auto)]
        iso_fix: Toggle,
        /// Wheel units per scroll step. Defaults to the device's `scroll_rate`.
        #[arg(long)]
        scroll_rate: Option<u32>,
        /// Preferred codec when the device offers both (defaults to device's video_format).
        #[arg(long, value_enum)]
        codec: Option<CodecArg>,
        /// How video reaches us: direct (`/api/media/ws`), pion (WebRTC FEC), janus (WebRTC).
        #[arg(long, value_enum, default_value_t = TransportArg::Direct)]
        transport: TransportArg,
        /// Play the target's audio (device in webrtc, flex_fec or direct_h264 mode; opens a second WebRTC connection).
        #[arg(long)]
        audio: bool,
        /// How decoded frames reach the GPU.
        #[arg(long, value_enum, default_value_t = RenderArg::Auto)]
        render: RenderArg,
        /// Measure CPU for this many seconds (after a 3 s warm-up), print the result and exit.
        #[arg(long, value_name = "SECONDS")]
        bench: Option<u64>,
    },
    /// Print protocol details: streamer state, media formats and the first frames.
    Probe {
        #[arg(long, default_value_t = 10)]
        frames: usize,
        /// Ask for a keyframe after this many frames, to see how fast the encoder answers.
        #[arg(long, value_name = "FRAME")]
        key_request_at: Option<usize>,
    },
    /// Model, firmware, features and target status.
    Info,
    /// Save a JPEG snapshot of the target's screen.
    Snapshot {
        #[arg(default_value = "snapshot.jpg")]
        out: PathBuf,
        /// Scale down to fit WIDTHxHEIGHT.
        #[arg(long, value_parser = parse_size)]
        preview: Option<(u32, u32)>,
    },
    /// ATX power action on the target (asks for confirmation).
    Power {
        #[arg(value_enum)]
        action: PowerArg,
        /// Don't ask for confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Press an ATX button (asks for confirmation).
    Click {
        #[arg(value_enum)]
        button: ButtonArg,
        #[arg(long)]
        yes: bool,
    },
    /// Show the device's saved video mode, or change it (device-wide, asks for confirmation).
    VideoMode {
        #[arg(value_enum)]
        mode: Option<VideoModeArg>,
        #[arg(long)]
        yes: bool,
    },
    /// Show the encoder codec, or change it (device-wide, asks for confirmation).
    Codec {
        /// Named format: h264 or h265.
        #[arg(value_enum)]
        format: Option<CodecArg>,
        #[arg(long)]
        yes: bool,
    },
    /// Show the encoder mode, or change it (device-wide, asks for confirmation).
    VencMode {
        #[arg(value_enum)]
        mode: Option<VencModeArg>,
        #[arg(long)]
        yes: bool,
    },
    /// Show the latency mode, or change it (device-wide, asks for confirmation).
    LatencyMode {
        #[arg(value_enum)]
        mode: Option<LatencyModeArg>,
        #[arg(long)]
        yes: bool,
    },
    /// Show the picture rotation, or change it (device-wide, asks for confirmation).
    Orientation {
        /// Degrees.
        #[arg(value_parser = clap::builder::PossibleValuesParser::new(["0", "90", "180", "270"]))]
        degrees: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// List the display (EDID) presets, or switch to one by key (device-wide, asks for
    /// confirmation; the target sees a monitor change).
    Edid {
        /// Preset key from the list, e.g. E1920x1080.
        key: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Show or change encoder quality (device-wide, asks for confirmation).
    Quality {
        /// Named bitrate level (inside the device limits). The frame rate is `--fps`.
        #[arg(value_enum)]
        preset: Option<PresetArg>,
        /// Frames per second.
        #[arg(long)]
        fps: Option<i64>,
        /// H.264 bitrate in kbit/s.
        #[arg(long)]
        bitrate: Option<i64>,
        /// Keyframe interval in frames.
        #[arg(long)]
        gop: Option<i64>,
        /// JPEG quality (1-100).
        #[arg(long)]
        jpeg_quality: Option<i64>,
        #[arg(long)]
        yes: bool,
    },
    /// Send a Wake-on-LAN packet to a MAC address or a saved target name.
    Wake { target: String },
    /// List saved Wake-on-LAN targets.
    WolList,
    /// Type text on the target (from the argument or stdin).
    Type {
        text: Option<String>,
        /// Target keyboard layout (see the device's keymaps); device default if omitted.
        #[arg(long)]
        keymap: Option<String>,
        /// Type slowly (BIOS/UEFI).
        #[arg(long)]
        slow: bool,
    },
    /// Record scrubbed read-only API responses as test fixtures.
    RecordFixtures { dir: PathBuf },
    /// Virtual media: images stored on the device, mounted as a CD-ROM or USB drive.
    Msd {
        #[command(subcommand)]
        cmd: MsdCmd,
    },
    /// Clipboard bridge on the device (docs/clipboard-bridge.md).
    Bridge {
        #[command(subcommand)]
        cmd: BridgeCmd,
    },
}

#[derive(Subcommand)]
pub(crate) enum MsdCmd {
    /// Show the stored images and what the target sees.
    Status,
    /// Upload an image file to the device's storage.
    Upload {
        file: PathBuf,
        /// Name on the device. Defaults to the file name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Connect an image to the target (device-wide, asks for confirmation).
    Mount {
        /// Image name as listed by `msd status`.
        image: String,
        /// Present it as a USB flash drive instead of a CD-ROM.
        #[arg(long)]
        flash: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Disconnect the drive from the target.
    Unmount {
        #[arg(long)]
        yes: bool,
    },
    /// Delete a stored image.
    Remove {
        image: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum BridgeCmd {
    /// Show whether the bridge is installed and healthy.
    Status,
    /// Install, update or repair the bridge, then upload the agent binaries.
    /// May restart the device's USB functions (the target's keyboard drops for a moment).
    Setup {
        /// Directory with agent binaries (scripts/build-agents.sh).
        #[arg(long, default_value = "dist/agents")]
        agents: PathBuf,
    },
    /// Remove the bridge and its USB network function from the device.
    Remove {
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum PowerArg {
    On,
    Off,
    OffHard,
    ResetHard,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum ButtonArg {
    Power,
    PowerLong,
    Reset,
}

fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s.split_once(['x', 'X']).ok_or("expected WIDTHxHEIGHT")?;
    Ok((
        w.parse().map_err(|_| "bad width")?,
        h.parse().map_err(|_| "bad height")?,
    ))
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum Toggle {
    Auto,
    On,
    Off,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum RenderArg {
    /// Zero-copy if available, otherwise upload.
    Auto,
    /// Copy each frame into GPU textures.
    Upload,
    /// Sample the decoder's IOSurfaces directly (macOS).
    ZeroCopy,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum PresetArg {
    Low,
    Medium,
    High,
    Max,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum VideoModeArg {
    /// WebRTC with FlexFEC (Pion).
    FlexFec,
    /// Classic WebRTC (Janus).
    Webrtc,
    /// GL native adaptive mode (disables Janus).
    GlWebrtc,
    DirectH264,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum VencModeArg {
    /// Reduces bandwidth.
    Smart,
    Normal,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum LatencyModeArg {
    LowestLatency,
    SmoothDisplay,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum TransportArg {
    Direct,
    /// WebRTC (FEC): GL's Pion server.
    Pion,
    /// WebRTC: Janus + uStreamer (needs `gl_webrtc` off).
    Janus,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum CodecArg {
    H264,
    H265,
}
