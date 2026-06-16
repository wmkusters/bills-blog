use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn run() {
    let window = wgpu::web_sys::window().unwrap();
    let document = window.document().unwrap();
    let text = document.get_element_by_id("foo").unwrap();
    text.set_text_content(Some("a string"));
}
