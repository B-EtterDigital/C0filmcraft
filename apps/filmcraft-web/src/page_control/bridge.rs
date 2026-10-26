use serde_json::json;
use std::{cell::Cell, rc::Rc};
use wasm_bindgen::{JsCast as _, prelude::*};
use wasm_bindgen_futures::{JsFuture, future_to_promise};

/// Reuse the browser API's sender, promise/error handling and reply polling. The event target
/// lends the same unpresented egui pass used by the other Craft guest pages; for `ui.*` requests
/// that pass is a UI pass even while the page is hidden (`super::lend_ui_pass`).
pub async fn attach(runner: &eframe::WebRunner) -> Result<(), JsValue> {
    #[cfg(not(feature = "patent-codecs"))]
    super::codecs::install().await;
    let window = web_sys::window().ok_or("no window")?;
    let api = js_sys::Reflect::get(&window, &"filmcraft".into())?;
    let request = js_sys::Reflect::get(&api, &"request".into())?.dyn_into::<js_sys::Function>()?;
    let target = web_sys::EventTarget::new()?;
    let ran = Rc::new(Cell::new(false));
    let signal = ran.clone();
    runner.add_event_listener::<web_sys::Event>(&target, "filmcraft-control-pass", move |_, app_runner| {
        app_runner.logic();
        signal.set(true);
    })?;
    let call = Closure::<dyn Fn(JsValue) -> js_sys::Promise>::new(move |text: JsValue| {
        let parsed = text.as_string().ok_or_else(|| "request must be JSON text".to_string()).and_then(|text| super::parse(&text));
        let (method, params) = match parsed {
            Ok(value) => value,
            Err(error) => return reply_error(error),
        };
        let picker_cancelled = method == "c0.cancelFileDialog" && super::files::cancel();
        let result = request.call2(&api, &method.clone().into(), &crate::api::to_js(&params)).and_then(|p| p.dyn_into::<js_sys::Promise>());
        let promise = match result {
            Ok(promise) => promise,
            Err(error) => return reply_error(message(error)),
        };
        ran.set(false);
        // `ui.*` requests are answered at the end of a UI pass: lend one to a hidden page.
        let lend = super::needs_ui_pass(&method);
        if let Ok(event) = web_sys::Event::new("filmcraft-control-pass") {
            super::lending_ui_pass(lend, || {
                let _ = target.dispatch_event(&event);
                for _ in 0..super::inline_passes(&method) {
                    let _ = target.dispatch_event(&event);
                }
            });
        }
        // Hidden tabs do not get animation frames. One queued event pass must have run now.
        if !ran.get() {
            return reply_error("FilmCraft runner is busy; retry the request");
        }
        let pump_target = target.clone();
        future_to_promise(async move {
            let pump = Closure::<dyn FnMut()>::new(move || {
                if let Ok(event) = web_sys::Event::new("filmcraft-control-pass") {
                    let _ = super::lending_ui_pass(lend, || pump_target.dispatch_event(&event));
                }
            });
            let interval =
                web_sys::window().and_then(|window| window.set_interval_with_callback_and_timeout_and_arguments_0(pump.as_ref().unchecked_ref(), 16).ok());
            let reply = match JsFuture::from(promise).await {
                Ok(result) => {
                    let mut result = crate::api::from_js(&result);
                    if picker_cancelled && let Some(result) = result.as_object_mut() {
                        result.insert("cancelled".into(), json!(true));
                    }
                    json!({"ok":true,"result":result})
                }
                Err(error) => json!({"ok":false,"error":message(error)}),
            };
            if let (Some(window), Some(interval)) = (web_sys::window(), interval) {
                window.clear_interval_with_handle(interval);
            }
            drop(pump);
            Ok(JsValue::from_str(&reply.to_string()))
        })
    });
    js_sys::Reflect::set(&window, &"__filmcraftControl".into(), &call.into_js_value())?;
    Ok(())
}

fn message(error: JsValue) -> String {
    js_sys::Reflect::get(&error, &"message".into()).ok().and_then(|v| v.as_string()).or_else(|| error.as_string()).unwrap_or_else(|| format!("{error:?}"))
}
fn reply_error(error: impl std::fmt::Display) -> js_sys::Promise {
    js_sys::Promise::resolve(&JsValue::from_str(&json!({"ok":false,"error":error.to_string()}).to_string()))
}
