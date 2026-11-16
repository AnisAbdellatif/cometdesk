//! `cometdesk-mock` binary: runs the mock device until Ctrl-C, for demos and manual GUI/CLI
//! testing. Flags: `--port N` (default random), `--atx`, `--twofa`. Read `lib.rs` for behaviour.

use cometdesk_mock::{Config, MockDevice, PASSWORD, TOTP, USER};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: cometdesk-mock [--port N] [--atx] [--twofa]";
    let mut port = 0u16;
    let (mut atx, mut twofa) = (false, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--atx" => atx = true,
            "--twofa" => twofa = true,
            "--port" => match it.next().and_then(|v| v.parse().ok()) {
                Some(p) => port = p,
                None => fail(&format!("--port needs a number (0-65535)\n{usage}")),
            },
            "-h" | "--help" => {
                println!("{usage}");
                return;
            }
            other => fail(&format!("unknown argument {other:?}\n{usage}")),
        }
    }
    let config = Config {
        atx,
        twofa,
        ..Config::default()
    };
    let dev = match MockDevice::start_on(config, &format!("127.0.0.1:{port}")).await {
        Ok(d) => d,
        Err(e) => fail(&format!("cannot listen on 127.0.0.1:{port}: {e}")),
    };
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

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}
