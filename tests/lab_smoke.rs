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
    let (status, page) = http(port, "GET", &format!("/?t={token}"), None, b"");
    assert_eq!(status, 200);
    assert!(page.contains(r#"src="/app.js""#) && !page.contains("<script>") && !page.contains("style="), "the page needs no inline script or style (strict CSP)");
    assert!(page.contains(token), "the session key is embedded for the page's own script");
    assert_eq!(http(port, "GET", "/app.js", None, b"").0, 200);
    assert_eq!(http(port, "GET", "/app.css", None, b"").0, 200);
    assert_eq!(http(port, "POST", "/api/checks?name=x", Some(token), b"not an nro").0, 400);
    let (status, config) = http(port, "GET", "/api/config", Some(token), b"");
    assert_eq!(status, 200, "{config}");
    assert!(serde_json::from_str::<serde_json::Value>(&config).unwrap()["default_budget"].as_u64().is_some());
    assert_eq!(http(port, "GET", "/api/config", None, b"").0, 403, "config needs the session key");
    let nro_head = [vec![0u8; 0x10], b"NRO0".to_vec()].concat();
    assert_eq!(http(port, "POST", "/api/checks?name=x.nro&engines=gpu", Some(token), &nro_head).0, 400, "unknown engine");
    assert_eq!(http(port, "POST", "/api/checks?name=x.nro&budget=0", Some(token), &nro_head).0, 400, "zero budget");

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
    assert!(check["instructions"].is_null(), "no instruction check unless the computer reference is on");
    assert!(dir.join(&id).join("result.json").exists(), "the result is saved");
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn server_exits_when_its_parent_pipe_closes() {
    let dir = std::env::temp_dir().join(format!("ounce-lab-pipe-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["ui", "--no-open", "--parent-pipe", "--dir"]).arg(&dir)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    BufReader::new(stdout).lines().map_while(Result::ok)
        .find(|l| l.contains("http://127.0.0.1:")).expect("the server starts");
    drop(child.stdin.take()); // what happens when the Mac app dies
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("the server outlived its parent pipe");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = std::fs::remove_dir_all(&dir);
}
