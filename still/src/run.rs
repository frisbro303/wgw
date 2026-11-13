//! Running a JavaScript widget in Boa, a JavaScript engine in Rust, through still.js and the page's own patch.js.

use std::path::Path;
use std::rc::Rc;

use boa_engine::builtins::promise::PromiseState;
use boa_engine::context::{time::FixedClock, ContextBuilder, HostHooks};
use boa_engine::module::{MapModuleLoader, Module};
use boa_engine::object::builtins::JsPromise;
use boa_engine::{js_string, Context, JsError, JsValue, Source};

const PATCH_JS: &str = include_str!("../../js/patch.js");
const STILL_JS: &str = include_str!("still.js");

// A plugin has no clock and no time zone: Date sees the Unix epoch, in UTC. Widgets drawing a still need neither.
struct Hooks;
impl HostHooks for Hooks {
    fn local_timezone_offset_seconds(&self, _unix_time_seconds: i64) -> i32 {
        0
    }
}

/// Run a widget module's source with the figure's mesh (as JSON, {name: {data, shape, dtype}}, or None), returning
/// what still.js's run() returns: JSON of the patches, the grid setting and the panes.
pub fn widget(source: &str, mesh: Option<&str>) -> Result<String, String> {
    let loader = Rc::new(MapModuleLoader::new());
    let mut context = ContextBuilder::default()
        .clock(Rc::new(FixedClock::default()))
        .host_hooks(Rc::new(Hooks))
        .module_loader(loader.clone())
        .build()
        .map_err(|e| e.to_string())?;
    let ctx = &mut context;
    let message = |e: JsError, ctx: &mut Context| -> String {
        match e.try_native(ctx) {
            Ok(native) => native.message().to_string(),
            Err(_) => e.to_string(),
        }
    };

    let parse = |path: &str, code: &str, ctx: &mut Context| -> Result<Module, String> {
        let module = Module::parse(Source::from_bytes(code).with_path(Path::new(path)), None, ctx)
            .map_err(|e| format!("{}: {}", path.trim_start_matches('/'), message(e, ctx)))?;
        loader.insert(path, module.clone());
        Ok(module)
    };
    parse("/wgw/patch.js", PATCH_JS, ctx)?;
    parse("/widget/widget.js", source, ctx).map_err(|e| e.replace("widget/widget.js", "the widget"))?;
    let still = parse("/wgw/still.js", STILL_JS, ctx)?;

    let loaded = still.load_link_evaluate(ctx);
    ctx.run_jobs().map_err(|e| message(e, ctx))?;
    if let PromiseState::Rejected(e) = loaded.state() {
        return Err(format!("the widget: {}", message(JsError::from_opaque(e), ctx)));
    }

    let run = still.namespace(ctx).get(js_string!("run"), ctx).map_err(|e| message(e, ctx))?;
    let run = run.as_callable().ok_or("still.js has no run()")?;
    let arg = mesh.map_or(JsValue::null(), |m| JsValue::from(js_string!(m)));
    let result = run.call(&JsValue::undefined(), &[arg], ctx).map_err(|e| message(e, ctx))?;
    let promise = result.as_object().and_then(|o| JsPromise::from_object(o.clone()).ok()).ok_or("run() didn't return a promise")?;
    ctx.run_jobs().map_err(|e| message(e, ctx))?;
    match promise.state() {
        PromiseState::Fulfilled(v) => Ok(v.to_string(ctx).map_err(|e| message(e, ctx))?.to_std_string_escaped()),
        PromiseState::Rejected(e) => Err(format!("the widget: {}", message(JsError::from_opaque(e), ctx))),
        PromiseState::Pending => Err("the widget never finished (it waits on something a still can't give it)".into()),
    }
}
