//! Encoder and streamer configuration subcommands (mode, codec, latency, rotation, quality).
//!
//! Entry points: `video_mode`, `venc_mode`, `latency_mode`, `codec`, `orientation`, `quality`.

use anyhow::{Result, bail};
use cometdesk::Session;
use cometdesk::api::streamer::{QualityPreset, StreamerUpdate};
use cometdesk::api::system::{LatencyMode, Orientation, VencMode, VideoFormat};

use super::util::confirm;

pub async fn video_mode(
    session: &Session,
    mode: Option<cometdesk::api::system::VideoMode>,
    yes: bool,
) -> Result<()> {
    let current = session.system_config().await?.video_mode;
    println!("video_mode: {}", current.as_deref().unwrap_or("unknown"));
    let Some(mode) = mode else { return Ok(()) };
    if current.as_deref() == Some(mode.wire_name()) {
        println!("already {}", mode.wire_name());
        return Ok(());
    }
    confirm(
        &format!(
            "Switch the device's video mode to \"{}\"? This is device-wide and persisted: \
             other viewers (web UI, other apps) are affected.",
            mode.wire_name()
        ),
        yes,
    )?;
    session.set_video_mode(mode).await?;
    println!("video_mode: {} (confirmed)", mode.wire_name());
    Ok(())
}

pub async fn venc_mode(session: &Session, mode: Option<VencMode>, yes: bool) -> Result<()> {
    let current = session.system_config().await?.venc_mode;
    println!("venc_mode: {}", current.as_deref().unwrap_or("unknown"));
    let Some(mode) = mode else { return Ok(()) };
    if current.as_deref() == Some(mode.wire_name()) {
        println!("already {}", mode.wire_name());
        return Ok(());
    }
    confirm(
        &format!(
            "Switch the encoder to \"{}\" mode? This is device-wide and persisted.",
            mode.wire_name()
        ),
        yes,
    )?;
    session.set_venc_mode(mode).await?;
    println!("venc_mode: {} (confirmed)", mode.wire_name());
    Ok(())
}

pub async fn latency_mode(session: &Session, mode: Option<LatencyMode>, yes: bool) -> Result<()> {
    let current = session.system_config().await?.video_processing;
    println!(
        "video_processing: {}",
        current.as_deref().unwrap_or("unknown")
    );
    let Some(mode) = mode else { return Ok(()) };
    if current.as_deref() == Some(mode.wire_name()) {
        println!("already {}", mode.wire_name());
        return Ok(());
    }
    confirm(
        &format!(
            "Switch the latency mode to \"{}\"? This is device-wide and persisted.",
            mode.wire_name()
        ),
        yes,
    )?;
    session.set_latency_mode(mode).await?;
    println!("video_processing: {} (confirmed)", mode.wire_name());
    Ok(())
}

pub async fn codec(session: &Session, format: Option<VideoFormat>, yes: bool) -> Result<()> {
    let current = session.video_format().await?;
    let st = session.streamer_state().await.ok();
    let h265_supported = st.as_ref().map(|s| s.features.h265).unwrap_or(false);
    println!(
        "codec: {} (H.265 supported: {})",
        current.map_or("unknown", |f| f.name()),
        if h265_supported { "yes" } else { "no" },
    );
    let Some(format) = format else { return Ok(()) };
    if current == Some(format) {
        println!("already {}", format.name());
        return Ok(());
    }
    if format == VideoFormat::H265 && !h265_supported {
        bail!("device does not support H.265 (features.h265 is false)");
    }
    confirm(
        &format!(
            "Switch the encoder codec to \"{}\"? This is device-wide and persisted: \
             other viewers are affected and the encoder restarts.",
            format.name(),
        ),
        yes,
    )?;
    session.set_video_format(format).await?;
    println!("codec: {} (confirmed)", format.name());
    Ok(())
}

pub async fn orientation(session: &Session, o: Option<Orientation>, yes: bool) -> Result<()> {
    let current = session.system_config().await?.orientation;
    println!(
        "orientation: {}",
        current.map_or("unknown".into(), |d| d.to_string())
    );
    let Some(o) = o else { return Ok(()) };
    if current == Some(o.degrees()) {
        println!("already {}", o.degrees());
        return Ok(());
    }
    confirm(
        &format!(
            "Rotate the picture to {}°? This is device-wide and persisted: other viewers \
             (web UI, other apps) are affected.",
            o.degrees()
        ),
        yes,
    )?;
    session.set_orientation(o).await?;
    println!("orientation: {} (confirmed)", o.degrees());
    Ok(())
}

pub async fn quality(
    session: &Session,
    preset: Option<QualityPreset>,
    manual: StreamerUpdate,
    yes: bool,
) -> Result<()> {
    let st = session.streamer_state().await?;
    let show = |st: &cometdesk::api::streamer::StreamerState| {
        let (p, l) = (&st.params, &st.limits);
        println!(
            "fps {:?} ({}-{}), bitrate {:?} kbit/s ({}-{}), gop {:?} ({}-{}), quality {:?}",
            p.desired_fps,
            l.desired_fps.min,
            l.desired_fps.max,
            p.h264_bitrate,
            l.h264_bitrate.min,
            l.h264_bitrate.max,
            p.h264_gop,
            l.h264_gop.min,
            l.h264_gop.max,
            p.quality
        );
    };
    show(&st);
    let mut update = StreamerUpdate {
        h264_bitrate: preset.map(|p| p.bitrate(&st.limits)),
        ..Default::default()
    };
    for (dst, src) in [
        (&mut update.desired_fps, manual.desired_fps),
        (&mut update.h264_bitrate, manual.h264_bitrate),
        (&mut update.h264_gop, manual.h264_gop),
        (&mut update.quality, manual.quality),
    ] {
        *dst = src.or(*dst);
    }
    if update == StreamerUpdate::default() {
        return Ok(());
    }
    let update = update.clamped(&st.limits);
    confirm(
        &format!(
            "Change the encoder to {update:?}? This is device-wide and persisted: every viewer is affected."
        ),
        yes,
    )?;
    show(&session.set_streamer_params(update).await?);
    Ok(())
}
