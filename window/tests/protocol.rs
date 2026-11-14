//! wgw-window as a head sees it (docs/heads.typ): served headless (--serve), so these run anywhere, windows or not.

use std::io::{BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

struct Head {
    child: Child,
    to: ChildStdin,
    from: BufReader<ChildStdout>,
}

impl Head {
    fn start(protocol: &str) -> Head {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wgw-window"))
            .args(["--protocol", protocol, "--serve"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let to = child.stdin.take().unwrap();
        let from = BufReader::new(child.stdout.take().unwrap());
        Head { child, to, from }
    }

    fn send(&mut self, header: Value, body: &[u8]) {
        let text = header.to_string();
        self.to.write_all(&(text.len() as u32).to_le_bytes()).unwrap();
        self.to.write_all(text.as_bytes()).unwrap();
        self.to.write_all(&(body.len() as u32).to_le_bytes()).unwrap();
        self.to.write_all(body).unwrap();
        self.to.flush().unwrap();
    }

    fn read(&mut self) -> (String, Vec<u8>) {
        let mut part = || {
            let mut n = [0u8; 4];
            self.from.read_exact(&mut n).unwrap();
            let mut bytes = vec![0u8; u32::from_le_bytes(n) as usize];
            self.from.read_exact(&mut bytes).unwrap();
            bytes
        };
        (String::from_utf8(part()).unwrap(), part())
    }
}

/// A patch: one mesh, a triangle.
fn triangle() -> Vec<u8> {
    use wgw_scene::array::{Data, NdArray};
    use wgw_scene::patch::{encode, LayerPatch, Patch};
    let layer = LayerPatch {
        kind: "mesh".into(),
        arrays: vec![
            ("vertices".into(), NdArray::new(Data::F32(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]), vec![3, 3]).unwrap()),
            ("faces".into(), NdArray::new(Data::U32(vec![0, 1, 2]), vec![1, 3]).unwrap()),
        ],
        ..Default::default()
    };
    encode(&Patch { layers: vec![("mesh".into(), Some(layer))] })
}

/// POST a state to a figure's scene, as the page does: the status and the body.
fn post(url: &str, state: &Value) -> (u16, Vec<u8>) {
    let rest = url.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_once('/').unwrap();
    let body = state.to_string();
    let mut s = TcpStream::connect(host).unwrap();
    write!(s, "POST /{path}/scene HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    let mut response = Vec::new();
    s.read_to_end(&mut response).unwrap();
    let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let status = String::from_utf8_lossy(&response[9..12]).parse().unwrap();
    (status, response[split + 4..].to_vec())
}

#[test]
fn a_function_figure_is_asked_for_its_scenes() {
    let mut head = Head::start("1");
    head.send(json!({ "type": "figure", "id": "7", "options": {}, "setup": { "controls": { "t": [0, 2, 1], "my box": true } }, "body": "callback" }), &[]);
    let (url, _) = head.read();
    let url = url.strip_prefix("url 7 ").expect("the figure's url").to_string();

    // The page asks; the head gets the state as lines, and answers with a patch, which the page gets.
    let page = std::thread::spawn({
        let url = url.clone();
        move || post(&url, &json!({ "controls": { "t": 1.5, "my box": false }, "picks": [{ "vertex": 3, "frame": 0, "point": [0.5, 0, 1], "layer": "the mesh" }] }))
    });
    let (request, state) = head.read();
    let words: Vec<&str> = request.split(' ').collect();
    assert_eq!((words[0], words[2]), ("scene", "7"));
    let state = String::from_utf8(state).unwrap();
    assert!(state.contains("control 1.5 t\n") && state.contains("control false my box\n"), "{state}");
    assert!(state.contains("pick 3 0 0.5 0 1 the mesh\n"), "{state}");
    head.send(json!({ "type": "scene", "request": words[1].parse::<u64>().unwrap() }), &triangle());
    assert_eq!(page.join().unwrap(), (200, triangle()));

    // A head's error reaches the page as the response's text.
    let page = std::thread::spawn(move || post(&url, &json!({ "controls": {} })));
    let (request, _) = head.read();
    let n: u64 = request.split(' ').nth(1).unwrap().parse().unwrap();
    head.send(json!({ "type": "error", "request": n, "message": "ValueError: no" }), &[]);
    assert_eq!(page.join().unwrap(), (500, b"ValueError: no".to_vec()));
    drop(head.to);
    assert!(head.child.wait().unwrap().success());
}

#[test]
fn saves_headless() {
    let dir = std::env::temp_dir().join(format!("wgw-window-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut head = Head::start("1");
    let figure = json!({ "width": 120, "height": 90, "scale": 1 });
    for (n, name) in ["still.png", "turn.gif", "turn.apng"].iter().enumerate() {
        let path = dir.join(name);
        head.send(json!({ "type": "save", "request": n, "path": path, "figure": figure, "video": { "frames": 3 }, "body": "scene", "setup": {} }), &triangle());
        assert_eq!(head.read().0, format!("saved {n}"));
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(if name.ends_with(".gif") { b"GIF89a" } else { b"\x89PNG" }), "{name}");
    }
    head.send(json!({ "type": "save", "request": 9, "path": dir.join("x.bmp"), "figure": figure, "body": "scene" }), &triangle());
    assert!(head.read().0.starts_with("failed 9 can't save as .bmp"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn another_protocol_is_refused() {
    let head = Head::start("2");
    let out = head.child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("protocol 2"));
}
