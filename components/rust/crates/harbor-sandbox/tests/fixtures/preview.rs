use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

fn main() {
    let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR").unwrap());
    if env::args().nth(1).as_deref() == Some("mock") {
        let listener = UnixListener::bind(runtime.join("mock.sock")).unwrap();
        for incoming in listener.incoming() {
            incoming.unwrap().write_all(b"synthetic").unwrap();
        }
        return;
    }
    let provider = loop {
        if let Ok(socket) = UnixStream::connect(runtime.join("mock.sock")) {
            break socket;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let mut response = String::new();
    (&provider).read_to_string(&mut response).unwrap();
    assert_eq!(response, "synthetic");
    fs::write(runtime.join("response"), response).unwrap();
    fs::write(runtime.join("version"), include_str!("design")).unwrap();
    loop {
        thread::sleep(Duration::from_millis(50));
    }
}
