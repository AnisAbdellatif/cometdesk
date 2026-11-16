//! `cometdesk-mock` binary: runs the mock device until Ctrl-C, for demos and manual GUI/CLI
//! testing. Flags: `--port N` (default random), `--atx`, `--twofa`. Read `lib.rs` for behaviour.

use cometdesk_mock::{Config, MockDevice, PASSWORD, TOTP, USER};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let port = args
        .iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .map_or("0", String::as_str);
    let config = Config {
        atx: flag("--atx"),
        twofa: flag("--twofa"),
        ..Config::default()
    };
    let twofa = config.twofa;
    let dev = MockDevice::start_on(config, &format!("127.0.0.1:{port}")).await;
    println!("mock GLKVM device on {} (self-signed TLS)", dev.host());
    println!("user: {USER}  password: {PASSWORD}");
    if twofa {
        println!("2FA code: {TOTP}");
    }
    println!(
        "CLI: COMETDESK_HOST={} COMETDESK_PASSWORD='{PASSWORD}' cometdesk-cli info",
        dev.host()
    );
    tokio::signal::ctrl_c().await.ok();
}
