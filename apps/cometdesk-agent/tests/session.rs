//! Runs the real agent program against this test acting as the GUI: browsing and fetching a
//! file, then replacing the agent with an "updated" copy and checking that it restarts and
//! reconnects. Unix only (it relies on the inode changing); uses the agent's instance port,
//! so it cannot run beside another agent.
#![cfg(unix)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::MetadataExt;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use cometdesk_clip::{DirTarget, Endpoint, Event, Hello, Purpose, Role, Secret};

struct Gui {
    ep: Endpoint,
    stream: TcpStream,
}

impl Gui {
    fn accept(listener: &TcpListener, secret: &Secret) -> Gui {
        let (stream, _) = listener.accept().expect("the agent connects");
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let ep = Endpoint::new(Role::Client, secret, Hello::local("test", "gui".into())).unwrap();
        let mut gui = Gui { ep, stream };
        gui.wait(|e| matches!(e, Event::Ready(_)));
        gui
    }

    /// Reads until an event satisfies `done`; returns every event seen up to then.
    fn wait(&mut self, mut done: impl FnMut(&Event) -> bool) -> Vec<Event> {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut seen = Vec::new();
        let mut buf = vec![0u8; 64 * 1024];
        while Instant::now() < deadline {
            if self.ep.has_outgoing() {
                self.stream.write_all(&self.ep.take_outgoing()).unwrap();
            }
            match self.stream.read(&mut buf) {
                Ok(0) => panic!("the agent closed the connection; saw {seen:?}"),
                Ok(n) => {
                    for e in self.ep.receive(&buf[..n]).unwrap() {
                        let stop = done(&e);
                        seen.push(e);
                        if stop {
                            if self.ep.has_outgoing() {
                                self.stream.write_all(&self.ep.take_outgoing()).unwrap();
                            }
                            return seen;
                        }
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => panic!("{e}"),
            }
        }
        panic!("timed out; saw {seen:?}");
    }
}

/// Stops the agent and the successor it started when it updated itself.
struct Kill(Child, std::path::PathBuf);
impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = Command::new("pkill").arg("-f").arg(&self.1).status();
    }
}

#[test]
fn browse_fetch_and_update() {
    let root = std::env::temp_dir().join(format!("cometdesk-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (config, bin_dir, data) = (root.join("home"), root.join("bin"), root.join("data"));
    for d in [&config, &bin_dir, &data] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(data.join("hello.txt"), "hello from the target").unwrap();
    let exe = bin_dir.join("cometdesk-agent");
    std::fs::copy(env!("CARGO_BIN_EXE_cometdesk-agent"), &exe).unwrap();

    let secret = Secret::generate();
    let paired = Command::new(&exe)
        .args(["pair", &secret.to_code()])
        .env("HOME", &config)
        .env("XDG_CONFIG_HOME", config.join(".config"))
        .status()
        .unwrap();
    assert!(paired.success());

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let relay = listener.local_addr().unwrap().to_string();
    let agent = Command::new(&exe)
        .args(["run", "--relay", &relay])
        .env("HOME", &config)
        .env("XDG_CONFIG_HOME", config.join(".config"))
        .spawn()
        .unwrap();
    let _guard = Kill(agent, exe.clone());

    let mut gui = Gui::accept(&listener, &secret);

    // Browse.
    let id = gui
        .ep
        .send_list_dir(DirTarget::Open(data.display().to_string()))
        .unwrap();
    let seen = gui.wait(|e| matches!(e, Event::DirList { id: i, .. } if *i == id));
    let Some(Event::DirList {
        result: Ok(list), ..
    }) = seen.last()
    else {
        panic!("{seen:?}")
    };
    assert_eq!(list.entries.len(), 1);
    assert_eq!(list.entries[0].name, "hello.txt");

    // Fetch: the agent offers the file with the Save purpose and streams it.
    let fetch = gui.ep.send_fetch(&list.path, "hello.txt").unwrap();
    let seen = gui.wait(|e| matches!(e, Event::FileEnd { .. }));
    assert!(seen.iter().any(|e| matches!(e, Event::FileOffer { purpose: Purpose::Save, name, .. } if name == "hello.txt")));
    let body: Vec<u8> = seen
        .iter()
        .filter_map(|e| match e {
            Event::FileData { data, .. } => Some(data.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(body, b"hello from the target");
    assert!(matches!(
        seen.last(),
        Some(Event::FileEnd { intact: true, .. })
    ));
    let _ = fetch;
    // Fetching something that is not a file is refused.
    let bad = gui.ep.send_fetch(&list.path, "../x").unwrap();
    let seen = gui.wait(|e| matches!(e, Event::Ack { id, .. } if *id == bad));
    assert!(matches!(
        seen.last(),
        Some(Event::Ack { result: Err(_), .. })
    ));

    // Update: send a program, get the verdict, and see the agent come back replaced.
    let before = std::fs::metadata(&exe).unwrap().ino();
    let program = std::fs::read(&exe).unwrap();
    let id = gui
        .ep
        .send_file_offer("cometdesk-agent", program.len() as u64, Purpose::Update)
        .unwrap();
    for chunk in program.chunks(cometdesk_clip::FILE_CHUNK) {
        gui.ep.send_file_data(id, chunk).unwrap();
    }
    gui.ep.send_file_end(id).unwrap();
    let seen = gui.wait(|e| matches!(e, Event::FileResult { .. }));
    assert!(
        matches!(seen.last(), Some(Event::FileResult { result: Ok(_), .. })),
        "{seen:?}"
    );
    drop(gui);
    let gui = Gui::accept(&listener, &secret);
    assert_ne!(std::fs::metadata(&exe).unwrap().ino(), before);
    assert!(!exe.with_file_name("cometdesk-agent.new").exists());
    drop(gui);
    let _ = std::fs::remove_dir_all(&root);
}
