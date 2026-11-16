//! End-to-end tests: the `cometdesk-cli` binary against the in-process mock device.
//! Entry points: the `#[tokio::test]`s; `run` is the shared harness.
//! Read `crates/cometdesk-mock/src/lib.rs` to change what the device answers.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use cometdesk_mock::{Config, MockDevice, PASSWORD, USER};

/// Per-test scratch dir used as HOME, XDG_CONFIG_HOME and cwd (keeps pins and `.env` isolated).
struct Home(PathBuf);

impl Home {
    fn new(name: &str) -> Home {
        let dir =
            std::env::temp_dir().join(format!("cometdesk-cli-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Home(dir)
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn run(dev: &MockDevice, home: &Home, password: &str, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cometdesk-cli"));
    cmd.args(args)
        .env("COMETDESK_HOST", dev.host())
        .env("COMETDESK_USER", USER)
        .env("COMETDESK_PASSWORD", password)
        .env_remove("COMETDESK_TOTP")
        .env("HOME", &home.0)
        .env("XDG_CONFIG_HOME", &home.0)
        .env("RUST_LOG", "warn")
        .current_dir(&home.0)
        .stdin(std::process::Stdio::null());
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// stderr, which carries the anyhow error chain.
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn atx() -> Config {
    Config {
        atx: true,
        ..Config::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn info_prints_model_and_features() {
    let (dev, home) = (
        MockDevice::start(Config::default()).await,
        Home::new("info"),
    );
    let o = run(&dev, &home, PASSWORD, &["info"]).await;
    assert!(o.status.success(), "{}", err(&o));
    let s = out(&o);
    assert!(s.contains("RM1"), "{s}");
    assert!(s.contains("kvmd"), "{s}");
    let features = s.lines().find(|l| l.starts_with("features")).unwrap();
    assert!(
        features.contains("hid") && features.contains("msd"),
        "{features}"
    );
    assert!(!features.contains("atx"), "{features}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_password_fails_login() {
    let (dev, home) = (
        MockDevice::start(Config::default()).await,
        Home::new("badpw"),
    );
    let o = run(&dev, &home, "nope", &["info"]).await;
    assert!(!o.status.success());
    assert!(err(&o).contains("wrong user or password"), "{}", err(&o));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn changed_certificate_is_refused() {
    let (dev, home) = (
        MockDevice::start(Config::default()).await,
        Home::new("cert"),
    );
    assert!(run(&dev, &home, PASSWORD, &["info"]).await.status.success());
    dev.rotate_cert();
    let o = run(&dev, &home, PASSWORD, &["info"]).await;
    assert!(!o.status.success());
    assert!(err(&o).contains("certificate for"), "{}", err(&o));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn power_needs_an_atx_board() {
    let (dev, home) = (
        MockDevice::start(Config::default()).await,
        Home::new("noatx"),
    );
    let o = run(&dev, &home, PASSWORD, &["power", "on", "--yes"]).await;
    assert!(!o.status.success());
    assert!(err(&o).contains("ATX is not enabled"), "{}", err(&o));
    assert!(dev.calls_to("/api/atx/power").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn power_without_yes_refuses_non_interactively() {
    let (dev, home) = (MockDevice::start(atx()).await, Home::new("noyes"));
    let o = run(&dev, &home, PASSWORD, &["power", "off"]).await;
    assert!(!o.status.success());
    assert!(err(&o).contains("pass --yes"), "{}", err(&o));
    assert!(dev.calls_to("/api/atx/power").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn power_sends_action() {
    let (dev, home) = (MockDevice::start(atx()).await, Home::new("power"));
    let o = run(&dev, &home, PASSWORD, &["power", "off-hard", "--yes"]).await;
    assert!(o.status.success(), "{}", err(&o));
    let calls = dev.calls_to("/api/atx/power");
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0].query.contains("action=off_hard"),
        "{}",
        calls[0].query
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn msd_status_lists_images() {
    let (dev, home) = (MockDevice::start(Config::default()).await, Home::new("msd"));
    let o = run(&dev, &home, PASSWORD, &["msd", "status"]).await;
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("image-0.iso"), "{}", out(&o));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn type_posts_text() {
    let (dev, home) = (
        MockDevice::start(Config::default()).await,
        Home::new("type"),
    );
    let o = run(&dev, &home, PASSWORD, &["type", "hello"]).await;
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("typed 5 characters"), "{}", out(&o));
    let calls = dev.calls_to("/api/hid/print");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].body, "hello");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn record_fixtures_round_trips() {
    let (dev, home) = (
        MockDevice::start(Config::default()).await,
        Home::new("record"),
    );
    let o = run(&dev, &home, PASSWORD, &["record-fixtures", "out"]).await;
    assert!(o.status.success(), "{}", err(&o));
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../cometdesk/tests/fixtures/rm1-v1.10.1");
    let read = |p: PathBuf| -> serde_json::Value {
        serde_json::from_slice(
            &std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())),
        )
        .unwrap()
    };
    let mut n = 0;
    for entry in std::fs::read_dir(&source).unwrap() {
        let name = entry.unwrap().file_name();
        assert_eq!(
            read(home.0.join("out").join(&name)),
            read(source.join(&name)),
            "{name:?}"
        );
        n += 1;
    }
    assert_eq!(n, 12);
}
