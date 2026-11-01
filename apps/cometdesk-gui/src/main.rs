//! cometdesk: native console for GL.iNet GLKVM devices.

mod actions;
mod app;
mod browser;
mod clipsync;
mod console;
mod crop;
mod devices;
mod gpu;
mod media;
mod ocr;
mod setup;
mod store;
mod sysnotify;
mod theme;
mod touchid;

use std::sync::Arc;

use anyhow::Result;
use cometdesk::tls::PinStore;
use winit::event_loop::EventLoop;

fn main() -> Result<()> {
    cometdesk::env::load_dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu=warn".into()),
        )
        .init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("cometdesk-io")
        .on_thread_start(cometdesk_view::set_user_interactive_qos)
        .enable_all()
        .build()?;
    let pins = Arc::new(PinStore::load_default()?);
    let iso_fix = cometdesk_view::keymap::host_keyboard_is_iso();

    let mut builder = EventLoop::<app::UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        // No default app menu: Cmd+Q, Cmd+H, ... go to the target while it has focus.
        builder.with_default_menu(false);
    }
    let event_loop = builder.build()?;
    app::control::start(event_loop.create_proxy());
    let mut app = app::App::new(rt, event_loop.create_proxy(), pins, iso_fix);
    let result = event_loop.run_app(&mut app);
    app.shutdown();
    result?;
    Ok(())
}
