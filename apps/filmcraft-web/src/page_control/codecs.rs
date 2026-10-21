//! WebCodecs adapters. JS handles stay on the UI thread; engine objects carry only numeric ids.
mod audio;
mod encode;
pub use audio::wrap;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/src/page_control/codecs.js")]
extern "C" {
    #[wasm_bindgen(catch,js_name=probeCodecs)]
    async fn probe_codecs() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=createVideo)]
    fn create_video(w: u32, h: u32, num: u32, den: u32, bitrate: u32, options: &JsValue) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=encodeVideo)]
    fn encode_video(state: &JsValue, bytes: &[u8], index: u32) -> Result<(), JsValue>;
    #[wasm_bindgen(catch,js_name=createAudio)]
    fn create_audio(rate: u32, channels: u32, bitrate: u32) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=encodeAudio)]
    fn encode_audio(state: &JsValue, planar: &[f32], frames: u32) -> Result<(), JsValue>;
    #[wasm_bindgen(catch,js_name=takePackets)]
    fn take_packets(state: &JsValue) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=encoderReady)]
    fn encoder_ready(state: &JsValue) -> Result<bool, JsValue>;
    #[wasm_bindgen(catch,js_name=encoderFinishReady)]
    fn encoder_finish_ready(state: &JsValue) -> Result<bool, JsValue>;
    #[wasm_bindgen(catch,js_name=closeEncoder)]
    fn close_encoder(state: &JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(catch,js_name=createAudioDecoder)]
    fn create_audio_decoder(description: &[u8], rate: u32, channels: u32) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=decodeAudio)]
    fn decode_audio(state: &JsValue, bytes: &[u8], timestamp: f64, duration: f64) -> Result<(), JsValue>;
    #[wasm_bindgen(catch,js_name=flushAudioDecoder)]
    fn flush_audio_decoder(state: &JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(catch,js_name=readAudio)]
    fn read_audio(state: &JsValue, start: f64, frames: u32, rate: u32) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=closeAudioDecoder)]
    fn close_audio_decoder(state: &JsValue) -> Result<(), JsValue>;
}
thread_local! {
    static NEXT:Cell<u32>=const{Cell::new(1)};
    static HANDLES:RefCell<HashMap<u32,JsValue>>=RefCell::new(HashMap::new());
    static CAPS:Cell<(bool,bool,bool)>=const{Cell::new((false,false,false))};
}
pub async fn install() {
    let disabled = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|search| web_sys::UrlSearchParams::new_with_str(&search).ok())
        .is_some_and(|params| params.has("nowebcodecs"));
    let caps = if disabled {
        (false, false, false)
    } else {
        match probe_codecs().await {
            Ok(value) => (
                get(&value, "video").as_bool().unwrap_or(false),
                get(&value, "audio").as_bool().unwrap_or(false),
                get(&value, "audioDecode").as_bool().unwrap_or(false),
            ),
            Err(_) => (false, false, false),
        }
    };
    CAPS.with(|slot| slot.set(caps));
    crate::set_info("browserCodecs", serde_json::json!({"h264Encoder":caps.0,"aacEncoder":caps.1,"aacDecoder":caps.2,"softwarePatentCodecs":false}));
    filmcraft_export::register_encoder(encode::video_factory);
    filmcraft_export::register_audio_encoder(encode::audio_factory);
    filmcraft_export::register_format_probe(filmcraft_export::Format::H264, || CAPS.with(|caps| caps.get().0));
    if caps.2 {
        filmcraft_codecs::register_reader_opener(crate::webcodecs::reader_opener);
    }
}
fn get(value: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(value, &key.into()).unwrap_or(JsValue::UNDEFINED)
}
fn bytes(value: &JsValue) -> Vec<u8> {
    if value.is_null() || value.is_undefined() { Vec::new() } else { js_sys::Uint8Array::new(value).to_vec() }
}
fn message(value: JsValue) -> String {
    get(&value, "message").as_string().or_else(|| value.as_string()).unwrap_or_else(|| format!("{value:?}"))
}
fn store(value: JsValue) -> Result<u32, String> {
    let id = NEXT.with(|slot| {
        let id = slot.get();
        if id != 0 {
            slot.set(id.checked_add(1).unwrap_or(0));
        }
        id
    });
    if id == 0 {
        return Err("browser codec handle limit reached".into());
    }
    HANDLES.with(|handles| handles.borrow_mut().insert(id, value));
    Ok(id)
}
fn with<T>(id: u32, f: impl FnOnce(&JsValue) -> Result<T, JsValue>) -> Result<T, String> {
    let value = HANDLES.with(|handles| handles.borrow().get(&id).cloned()).ok_or("browser codec has closed")?;
    f(&value).map_err(message)
}
fn close(id: u32, decoder: bool) {
    if let Some(value) = HANDLES.with(|handles| handles.borrow_mut().remove(&id)) {
        let _ = if decoder { close_audio_decoder(&value) } else { close_encoder(&value) };
    }
}
fn packets(id: u32) -> Result<Vec<JsValue>, String> {
    with(id, take_packets).map(|value| js_sys::Array::from(&value).iter().collect())
}
fn description(id: u32) -> Vec<u8> {
    with(id, |value| Ok(bytes(&get(value, "description")))).unwrap_or_default()
}
