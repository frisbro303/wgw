//! Each figure in a window of its own, with a webview (the "window" feature): tao for the windows, wry for the
//! webviews. The event loop runs on the main thread, as macOS needs.

use std::collections::HashMap;
use std::sync::Arc;

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};

use crate::{Open, Shared};

enum UserEvent {
    Open(Open),
    HeadGone,
}

pub struct Windows(EventLoop<UserEvent>);

impl Windows {
    pub fn new() -> Self {
        Windows(EventLoopBuilder::<UserEvent>::with_user_event().build())
    }

    /// Open a figure's window, from any thread.
    pub fn opener(&self) -> Box<dyn Fn(Open) + Send> {
        let proxy: EventLoopProxy<UserEvent> = self.0.create_proxy();
        Box::new(move |open| {
            let _ = proxy.send_event(UserEvent::Open(open));
        })
    }

    /// Close every window and exit: the head has gone.
    pub fn closer(&self) -> Box<dyn Fn() + Send> {
        let proxy = self.0.create_proxy();
        Box::new(move || {
            let _ = proxy.send_event(UserEvent::HeadGone);
        })
    }

    /// Run the windows until the head has gone, telling it when one is closed.
    pub fn run(self, shared: Arc<Shared>) -> ! {
        let mut open: HashMap<tao::window::WindowId, (String, tao::window::Window, wry::WebView)> = HashMap::new();
        let mut opened = 0u32;
        self.0.run(move |event, target, flow| {
            *flow = ControlFlow::Wait;
            match event {
                Event::UserEvent(UserEvent::Open(Open { id, title, url, height })) => {
                    let at = 80.0 + 30.0 * (opened % 10) as f64; // cascaded
                    let window = tao::window::WindowBuilder::new()
                        .with_title(title)
                        .with_inner_size(tao::dpi::LogicalSize::new(900.0, height))
                        .with_position(tao::dpi::LogicalPosition::new(at, at))
                        .build(target)
                        .expect("wgw-window: can't open a window");
                    match wry::WebViewBuilder::new().with_url(url).build(&window) {
                        Ok(webview) => {
                            opened += 1;
                            open.insert(window.id(), (id, window, webview));
                        }
                        Err(e) => eprintln!("wgw-window: can't make a webview: {e}"),
                    }
                }
                Event::UserEvent(UserEvent::HeadGone) => *flow = ControlFlow::Exit,
                Event::WindowEvent { event: WindowEvent::CloseRequested, window_id, .. } => {
                    if let Some((id, ..)) = open.remove(&window_id) {
                        shared.send(&format!("closed {id}"), &[]);
                    }
                }
                _ => {}
            }
        })
    }
}
