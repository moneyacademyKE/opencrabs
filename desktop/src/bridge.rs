use serde::Serialize;
use serde::de::DeserializeOwned;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure, prelude::*};

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn tauri_invoke_without_args(cmd: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn tauri_invoke_with_args(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = listen, catch)]
    async fn tauri_listen(event: &str, handler: &js_sys::Function) -> Result<JsValue, JsValue>;
}

pub async fn invoke<R, A>(cmd: &str, args: A) -> Result<R, String>
where
    R: DeserializeOwned + 'static,
    A: Serialize,
{
    #[cfg(target_arch = "wasm32")]
    {
        let args_value = serde_wasm_bindgen::to_value(&args).map_err(|e| e.to_string())?;
        let has_args = args_value
            .dyn_ref::<js_sys::Object>()
            .is_none_or(|object| js_sys::Object::keys(object).length() > 0);
        let value = if has_args {
            tauri_invoke_with_args(cmd, args_value)
                .await
                .map_err(js_error_to_string)?
        } else {
            tauri_invoke_without_args(cmd)
                .await
                .map_err(js_error_to_string)?
        };
        serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (cmd, args);
        Err("Desktop bridge is only available in the wasm frontend".to_string())
    }
}

pub async fn invoke_unit<A>(cmd: &str, args: A) -> Result<(), String>
where
    A: Serialize,
{
    invoke::<serde_json::Value, _>(cmd, args).await.map(|_| ())
}

#[cfg(target_arch = "wasm32")]
pub async fn listen(event: &str, callback: impl FnMut(JsValue) + 'static) -> Result<(), String> {
    let handler = Closure::wrap(Box::new(callback) as Box<dyn FnMut(JsValue)>);
    tauri_listen(event, handler.as_ref().unchecked_ref())
        .await
        .map_err(js_error_to_string)?;
    handler.forget();
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn event_payload<T: DeserializeOwned>(event: JsValue) -> Result<T, String> {
    let payload =
        js_sys::Reflect::get(&event, &JsValue::from_str("payload")).map_err(js_error_to_string)?;
    serde_wasm_bindgen::from_value(payload).map_err(|error| error.to_string())
}

#[cfg(target_arch = "wasm32")]
fn js_error_to_string(err: JsValue) -> String {
    err.as_string().unwrap_or_else(|| format!("{err:?}"))
}
