//! Running a widget (a .wgw file's WebAssembly module) as the page does, to draw it (the "widgets" feature): in
//! wasmi, a WebAssembly interpreter in Rust.

use serde_json::{json, Value};
use wasmi::{Caller, Engine, Extern, Instance, Linker, Module, Store, Val};
use wgw_scene::patch;

/// A widget's setup, and its scene for the controls given (every frame, stacked), as the page gets them.
pub fn run(bytes: &[u8], controls: &Value) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut w = Widget::new(bytes)?;
    let setup = w.call("setup", &[&[]])?;
    let frames = serde_json::from_slice::<Value>(&setup).ok().and_then(|s| s["frames"].as_u64()).unwrap_or(1).max(1);
    let controls = if controls.is_object() { controls.clone() } else { json!({}) };
    let patches = (0..frames)
        .map(|k| {
            let state = json!({ "controls": controls, "picks": [], "frame": k }).to_string();
            patch::decode(&w.call("scene", &[&[], state.as_bytes()])?)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((setup, patch::encode(&patch::stack(patches))))
}

#[derive(Default)]
struct Host {
    args: Vec<u8>,
    result: Vec<u8>,
}

/// A widget module, with Typst's plugin protocol: its functions take their arguments' lengths, read the bytes with
/// wasm_minimal_protocol_write_args_to_buffer, and give their result with wasm_minimal_protocol_send_result_to_host.
struct Widget {
    store: Store<Host>,
    instance: Instance,
}

impl Widget {
    fn new(bytes: &[u8]) -> Result<Widget, String> {
        let engine = Engine::default();
        let module = Module::new(&engine, bytes).map_err(|e| format!("the widget: {e}"))?;
        let mut linker = Linker::<Host>::new(&engine);
        let memory = |caller: &Caller<Host>| caller.get_export("memory").and_then(Extern::into_memory);
        linker
            .func_wrap("typst_env", "wasm_minimal_protocol_write_args_to_buffer", move |mut caller: Caller<Host>, ptr: i32| {
                let args = std::mem::take(&mut caller.data_mut().args);
                if let Some(m) = memory(&caller) {
                    let at = ptr as usize;
                    if let Some(to) = m.data_mut(&mut caller).get_mut(at..at + args.len()) {
                        to.copy_from_slice(&args);
                    }
                }
            })
            .and_then(|l| {
                l.func_wrap("typst_env", "wasm_minimal_protocol_send_result_to_host", move |mut caller: Caller<Host>, ptr: i32, len: i32| {
                    if let Some(m) = memory(&caller) {
                        let (at, n) = (ptr as usize, len as usize);
                        let result = m.data(&caller).get(at..at + n).map(<[u8]>::to_vec).unwrap_or_default();
                        caller.data_mut().result = result;
                    }
                })
            })
            .map_err(|e| e.to_string())?;
        let mut store = Store::new(&engine, Host::default());
        let instance = linker.instantiate_and_start(&mut store, &module).map_err(|e| format!("the widget: {e}"))?;
        Ok(Widget { store, instance })
    }

    fn call(&mut self, name: &str, parts: &[&[u8]]) -> Result<Vec<u8>, String> {
        let f = self.instance.get_func(&self.store, name).ok_or_else(|| format!("the widget has no {name}()"))?;
        *self.store.data_mut() = Host { args: parts.concat(), result: Vec::new() };
        let params: Vec<Val> = parts.iter().map(|p| Val::I32(p.len() as i32)).collect();
        let mut code = [Val::I32(0)];
        let called = f.call(&mut self.store, &params, &mut code);
        let result = std::mem::take(&mut self.store.data_mut().result);
        let text = || String::from_utf8_lossy(&result).into_owned();
        match called {
            Err(trap) => Err(if result.is_empty() { format!("the widget stopped: {trap}") } else { text() }),
            Ok(()) if code[0].i32() != Some(0) => Err(text()),
            Ok(()) => Ok(result),
        }
    }
}
