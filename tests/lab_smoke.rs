//! End to end: start `ounce-core ui`, upload a real NRO over HTTP, wait for the verdict.
//! Emulator engines only (no host reference), so it runs on every CI host.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn http(port: u16, method: &str, path: &str, token: Option<&str>, body: &[u8]) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    if let Some(t) = token {
        head.push_str(&format!("X-Ounce-Token: {t}\r\n"));
    }
    s.write_all(head.as_bytes()).unwrap();
    s.write_all(b"\r\n").unwrap();
    s.write_all(body).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let status = out.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (status, out.split_once("\r\n\r\n").map(|x| x.1.to_string()).unwrap_or_default())
}

#[test]
fn uploads_a_program_and_reports_a_verdict() {
    let dir = std::env::temp_dir().join(format!("ounce-lab-smoke-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["ui", "--no-open", "--dir"]).arg(&dir)
        .stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let server = Server(child);
    let url = BufReader::new(stdout).lines().map_while(Result::ok)
        .find_map(|l| l.split_once("http://127.0.0.1:").map(|(_, r)| r.to_string()))
        .expect("the server prints its address");
    let (port, token) = url.split_once("/?t=").unwrap();
    let port: u16 = port.parse().unwrap();

    assert_eq!(http(port, "GET", "/api/checks", None, b"").0, 403, "API needs the session key");
    assert_eq!(http(port, "GET", "/", None, b"").0, 403, "page needs the session key");
    assert_eq!(http(port, "GET", &format!("/?t={token}"), None, b"").0, 200);
    assert_eq!(http(port, "POST", "/api/checks?name=x", Some(token), b"not an nro").0, 400);

    let nro = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/hello/hello.nro")).unwrap();
    let (status, body) = http(port, "POST", "/api/checks?name=hello.nro", Some(token), &nro);
    assert_eq!(status, 202, "{body}");
    let id = serde_json::from_str::<serde_json::Value>(&body).unwrap()["id"].as_str().unwrap().to_string();

    let deadline = Instant::now() + Duration::from_secs(120);
    let check = loop {
        let (_, body) = http(port, "GET", &format!("/api/checks/{id}"), Some(token), b"");
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        if value["status"] == "done" {
            break value;
        }
        assert!(Instant::now() < deadline, "check did not finish: {body}");
        std::thread::sleep(Duration::from_millis(200));
    };
    assert_eq!(check["verdict"]["kind"], "agrees", "{check}");
    assert_eq!(check["verdict"]["reference"], false);
    let engines = check["engines"].as_array().unwrap();
    assert_eq!(engines.len(), 2);
    assert!(engines.iter().all(|e| e["stop"]["kind"] == "exited"), "{check}");
    assert!(dir.join(&id).join("result.json").exists(), "the result is saved");
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}
