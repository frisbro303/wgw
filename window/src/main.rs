//! wgw-window: shows wgw figures for wgw's heads (Python, Julia, R, Lean, Rust), each in a window with a webview,
//! or, with `--serve`, only serves them (for Jupyter, which shows them inline). A head starts it once and talks to it
//! over stdin and stdout, or, with `--connect <port>`, over a TCP connection to that port on 127.0.0.1 (for R, which
//! can't pipe both ways to a program); the page code, the core and any widget run in the webview, as on any page.
//!
//! Messages, both ways, are frames: a little-endian u32 length and a header, then a u32 length and a body. The
//! head's headers are JSON; this program's are a line of words, and a state is lines, so a head needs no JSON reader.
//!
//! From the head:
//!   {"type": "figure", "id", "title", "options", "setup", "height", "body": "src" | "scene" | "callback"}
//!       body "src": a .wgw file (a widget, or a mesh); "scene": a patch, the whole figure; "callback": none, and
//!       each scene is asked of the head
//!   {"type": "scene", "request"}              the patch asked for, as the body
//!   {"type": "error", "request", "message"}   the head couldn't make it
//!   {"type": "save", "request", "path", "figure": {width, height, scale, up, view, flat, background},
//!    "video": {fps, frames, seconds, spin}, "controls", "setup", "body": "src" | "scene"}
//!       save the figure as an image (.png) or a video (.gif, .apng; .mp4, .webm, .mov by ffmpeg), headless, by
//!       wgw-headless. The body is the .wgw file, or the patch to draw.
//! To the head:
//!   url <id> <url>                             where the figure is served
//!   scene <request> <id>                       a callback figure's scene, for the state in the body, a line each:
//!                                                control <value> <name>     a number, or true or false
//!                                                pick <vertex> <frame> <x> <y> <z> <layer>   the latest last
//!   closed <id>                                its window was closed
//!   saved <request>                            the figure was saved
//!   failed <request> <message>                 it couldn't be
//!
//! It exits, closing its windows, when the head does (its stdin closes).

use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use serde_json::{json, Value};

#[cfg(feature = "window")]
mod windows;

/// wgw's page code and core, built into dist/ by build.sh before this is compiled.
static FILES: &[(&str, &[u8], &str)] = &[
    ("wgw.js", include_bytes!("../../dist/wgw.js"), "text/javascript"),
    ("io.js", include_bytes!("../../dist/io.js"), "text/javascript"),
    ("patch.js", include_bytes!("../../dist/patch.js"), "text/javascript"),
    ("style.js", include_bytes!("../../dist/style.js"), "text/javascript"),
    ("widget.js", include_bytes!("../../dist/widget.js"), "text/javascript"),
    ("widget-worker.js", include_bytes!("../../dist/widget-worker.js"), "text/javascript"),
    ("logo.svg", include_bytes!("../../dist/logo.svg"), "image/svg+xml"),
    ("core/wgw_core.js", include_bytes!("../../dist/core/wgw_core.js"), "text/javascript"),
    ("core/wgw_core_bg.wasm", include_bytes!("../../dist/core/wgw_core_bg.wasm"), "application/wasm"),
];

const BAR: f64 = 40.0; // the controls' row, under a figure that has controls

enum Body {
    Src(Vec<u8>),
    Scene(Vec<u8>),
    Callback,
}

struct Figure {
    id: String,
    options: Value,
    setup: Value,
    body: Body,
}

type Reply = mpsc::Sender<Result<Vec<u8>, String>>;

pub struct Shared {
    figures: Mutex<HashMap<String, Arc<Figure>>>, // by the token in their URL
    pending: Mutex<HashMap<u64, Reply>>,          // callbacks waiting on the head
    next: AtomicU64,
    out: Mutex<Box<dyn Write + Send>>,
    head_gone: AtomicBool,
}

impl Shared {
    pub fn send(&self, header: &str, body: &[u8]) {
        let mut out = self.out.lock().unwrap();
        let text = header;
        let _ = out
            .write_all(&(text.len() as u32).to_le_bytes())
            .and_then(|_| out.write_all(text.as_bytes()))
            .and_then(|_| out.write_all(&(body.len() as u32).to_le_bytes()))
            .and_then(|_| out.write_all(body))
            .and_then(|_| out.flush());
    }
}

fn read_frame(input: &mut impl Read) -> io::Result<(Value, Vec<u8>)> {
    let mut part = || -> io::Result<Vec<u8>> {
        let mut n = [0u8; 4];
        input.read_exact(&mut n)?;
        let mut bytes = vec![0u8; u32::from_le_bytes(n) as usize];
        input.read_exact(&mut bytes)?;
        Ok(bytes)
    };
    let header = serde_json::from_slice(&part()?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok((header, part()?))
}

fn token() -> String {
    format!("{:016x}", RandomState::new().build_hasher().finish())
}

/// A window to open for a figure.
pub struct Open {
    pub id: String,
    pub title: String,
    pub url: String,
    pub height: f64,
}

/// The protocol heads speak, as they pass it (--protocol 1): a head and a wgw-window that don't agree stop at once.
const PROTOCOL: &str = "1";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).map(|i| args.get(i + 1).cloned().unwrap_or_default());
    if let Some(p) = arg("--protocol").filter(|p| p != PROTOCOL) {
        eprintln!("wgw-window {}: this head speaks protocol {p}, and this wgw-window protocol {PROTOCOL}: install matching versions", env!("CARGO_PKG_VERSION"));
        std::process::exit(2);
    }
    let serve_only = args.iter().any(|a| a == "--serve");
    #[cfg(not(feature = "window"))]
    if !serve_only {
        eprintln!("wgw-window was built without windows (the \"window\" feature): it can save figures, and serve them with --serve");
        std::process::exit(2);
    }
    // The head: stdin and stdout, or a connection to it.
    let (input, output): (Box<dyn Read + Send>, Box<dyn Write + Send>) = match arg("--connect") {
        Some(port) => {
            let port = port.parse::<u16>().expect("wgw-window: --connect <port>");
            let stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("wgw-window: can't connect to the head");
            (Box::new(stream.try_clone().unwrap()), Box::new(stream))
        }
        None => (Box::new(io::stdin()), Box::new(io::stdout())),
    };
    let shared = Arc::new(Shared {
        figures: Mutex::default(),
        pending: Mutex::default(),
        next: AtomicU64::new(0),
        out: Mutex::new(output),
        head_gone: AtomicBool::new(false),
    });

    let server = tiny_http::Server::http("127.0.0.1:0").expect("wgw-window: can't serve on 127.0.0.1");
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let http = shared.clone();
    thread::spawn(move || {
        for request in server.incoming_requests() {
            let shared = http.clone();
            thread::spawn(move || answer(&shared, request));
        }
    });

    // Windows, unless only serving: how to open one, and to close them all when the head goes.
    #[cfg(feature = "window")]
    let windows = (!serve_only).then(windows::Windows::new);
    #[cfg(feature = "window")]
    let (open, gone) = match &windows {
        Some(w) => (Some(w.opener()), Some(w.closer())),
        None => (None, None),
    };
    #[cfg(not(feature = "window"))]
    let (open, gone): (Option<Box<dyn Fn(Open) + Send>>, Option<Box<dyn Fn() + Send>>) = (None, None);

    // The head's messages.
    let reader = shared.clone();
    let input = thread::spawn(move || {
        let mut input = io::BufReader::new(input);
        while let Ok((header, body)) = read_frame(&mut input) {
            match header["type"].as_str() {
                Some("figure") => {
                    let id = header["id"].as_str().unwrap_or("").to_string();
                    let body = match header["body"].as_str() {
                        Some("src") => Body::Src(body),
                        Some("scene") => Body::Scene(body),
                        _ => Body::Callback,
                    };
                    let figure = Figure { id: id.clone(), options: header["options"].clone(), setup: header["setup"].clone(), body };
                    let url = format!("{base}/f/{}", token());
                    let key = url.rsplit('/').next().unwrap().to_string();
                    reader.figures.lock().unwrap().insert(key, Arc::new(figure));
                    reader.send(&format!("url {id} {url}"), &[]);
                    if let Some(open) = &open {
                        let title = header["title"].as_str().unwrap_or("wgw").to_string();
                        let none = header["body"] != "src" && header["setup"]["controls"].as_object().is_none_or(|c| c.is_empty());
                        let height = header["height"].as_f64().unwrap_or(420.0) + if none { 0.0 } else { BAR };
                        open(Open { id, title, url, height });
                    }
                }
                Some("save") => {
                    let saver = reader.clone();
                    thread::spawn(move || {
                        let request = header["request"].as_u64().unwrap_or(0);
                        match save(&header, &body) {
                            Ok(()) => saver.send(&format!("saved {request}"), &[]),
                            Err(message) => saver.send(&format!("failed {request} {message}"), &[]),
                        }
                    });
                }
                Some("scene") | Some("error") => {
                    let request = header["request"].as_u64().unwrap_or(u64::MAX);
                    if let Some(reply) = reader.pending.lock().unwrap().remove(&request) {
                        let _ = reply.send(if header["type"] == "scene" {
                            Ok(body)
                        } else {
                            Err(header["message"].as_str().unwrap_or("the scene failed").to_string())
                        });
                    }
                }
                _ => {}
            }
        }
        // The head has gone: callbacks can't be answered any more.
        reader.head_gone.store(true, Ordering::SeqCst);
        for (_, reply) in reader.pending.lock().unwrap().drain() {
            let _ = reply.send(Err(STOPPED.into()));
        }
        if let Some(gone) = &gone {
            gone();
        }
    });

    #[cfg(feature = "window")]
    if let Some(windows) = windows {
        windows.run(shared);
    }
    let _ = input.join(); // serving only: until the head goes
}

const STOPPED: &str = "the program that made this figure has stopped";

/// The page code, each figure's page, its file and its scenes.
fn answer(shared: &Shared, mut request: tiny_http::Request) {
    use tiny_http::{Header, Method, Response};
    let reply = |request: tiny_http::Request, code: u16, body: Vec<u8>, kind: &str| {
        let response = Response::from_data(body)
            .with_status_code(code)
            .with_header(Header::from_bytes("Content-Type", kind).unwrap())
            .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap());
        let _ = request.respond(response);
    };
    let path = request.url().split('?').next().unwrap_or("").to_string();
    if let Some(name) = path.strip_prefix("/wgw/") {
        return match FILES.iter().find(|(n, ..)| *n == name) {
            Some((_, bytes, kind)) => reply(request, 200, bytes.to_vec(), kind),
            None => reply(request, 404, b"not found".to_vec(), "text/plain"),
        };
    }
    let parts: Vec<&str> = path.trim_start_matches("/f/").splitn(2, '/').collect();
    let figure = shared.figures.lock().unwrap().get(parts[0]).cloned();
    let Some(figure) = figure.filter(|_| path.starts_with("/f/")) else {
        return reply(request, 404, b"not found".to_vec(), "text/plain");
    };
    let at = format!("/f/{}", parts[0]);
    match (parts.get(1).copied(), request.method()) {
        (None, Method::Get) => reply(request, 200, page(&figure, &at).into_bytes(), "text/html; charset=utf-8"),
        (Some("src"), Method::Get) => match &figure.body {
            Body::Src(bytes) => reply(request, 200, bytes.clone(), "application/octet-stream"),
            _ => reply(request, 404, b"not found".to_vec(), "text/plain"),
        },
        (Some("scene"), Method::Post) => match &figure.body {
            Body::Scene(bytes) => reply(request, 200, bytes.clone(), "application/octet-stream"),
            Body::Callback => {
                let mut state = String::new();
                let _ = request.as_reader().read_to_string(&mut state);
                let state: Value = serde_json::from_str(&state).unwrap_or(json!({}));
                let result = if shared.head_gone.load(Ordering::SeqCst) {
                    Err(STOPPED.to_string())
                } else {
                    let (tx, rx) = mpsc::channel();
                    let n = shared.next.fetch_add(1, Ordering::SeqCst);
                    shared.pending.lock().unwrap().insert(n, tx);
                    shared.send(&format!("scene {n} {}", figure.id), state_lines(&state).as_bytes());
                    rx.recv().unwrap_or_else(|_| Err(STOPPED.to_string()))
                };
                match result {
                    Ok(bytes) => reply(request, 200, bytes, "application/octet-stream"),
                    Err(message) => reply(request, 500, message.into_bytes(), "text/plain; charset=utf-8"),
                }
            }
            Body::Src(_) => reply(request, 404, b"not found".to_vec(), "text/plain"),
        },
        _ => reply(request, 404, b"not found".to_vec(), "text/plain"),
    }
}

/// Save what a head asks (see the top): wgw-headless draws and writes it.
fn save(header: &Value, body: &[u8]) -> Result<(), String> {
    let path = header["path"].as_str().ok_or("save: no path")?;
    let figure: wgw_headless::Figure = serde_json::from_value(header["figure"].clone()).map_err(|e| format!("save: {e}"))?;
    let video: wgw_headless::Video = serde_json::from_value(header["video"].clone()).unwrap_or_default();
    let setup = header["setup"].to_string();
    let source = match header["body"].as_str() {
        Some("src") => wgw_headless::Source::File(body),
        _ => wgw_headless::Source::Scene { setup: setup.as_bytes(), patch: body },
    };
    wgw_headless::save(std::path::Path::new(path), figure, &video, source, &header["controls"])
}

/// The page's state ({controls, picks}) as lines: `control <value> <name>`, `pick <vertex> <frame> <x> <y> <z> <layer>`.
fn state_lines(state: &Value) -> String {
    let mut out = String::new();
    for (name, value) in state["controls"].as_object().into_iter().flatten() {
        out += &format!("control {value} {name}\n");
    }
    for p in state["picks"].as_array().into_iter().flatten() {
        let at = |i: usize| p["point"][i].as_f64().map_or("nan".to_string(), |x| x.to_string());
        out += &format!("pick {} {} {} {} {} {}\n", p["vertex"], p["frame"].as_u64().unwrap_or(0), at(0), at(1), at(2), p["layer"].as_str().unwrap_or(""));
    }
    out
}

/// A figure's page: the figure filling it, its controls under it.
fn page(figure: &Figure, at: &str) -> String {
    let source = match figure.body {
        Body::Src(_) => format!("src: {:?}", format!("{at}/src")),
        _ => format!("widget: remoteWidget({:?}, {})", format!("{at}/scene"), figure.setup),
    };
    format!(
        r#"<!doctype html>
<meta charset="utf-8">
<title>wgw</title>
<style>
  :root {{ color-scheme: light dark; --wgw-colors: #dad8ce, #af3029; --wgw-edge: rgba(16, 15, 15, 0.25); }}
  @media (prefers-color-scheme: dark) {{ :root {{ --wgw-colors: #403e3c, #d14d41; --wgw-edge: rgba(255, 255, 255, 0.15); }} }}
  body {{ margin: 0; overflow: hidden; }}
  .wgw-controls {{ padding: 10px 0; }}
</style>
<div id="figure"></div>
<script type="module">
  import {{ mount }} from "/wgw/wgw.js";
  import {{ remoteWidget }} from "/wgw/widget.js";
  const figure = mount(document.getElementById("figure"), {{ ...{options}, {source} }});
  // The figure fills the window, above its controls (a widget's are only known once it has loaded).
  const fit = () => document.documentElement.style.setProperty("--wgw-height", `${{innerHeight - figure.controlsEl.offsetHeight}}px`);
  new ResizeObserver(fit).observe(figure.controlsEl);
  addEventListener("resize", fit);
  fit();
</script>
"#,
        options = if figure.options.is_object() { figure.options.to_string() } else { "{}".into() },
    )
}
