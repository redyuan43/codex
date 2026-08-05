use super::image_fallback_model;
use codex_protocol::user_input::UserInput;
use pretty_assertions::assert_eq;

#[test]
fn spark_routes_image_input_to_luna() {
    let items = vec![UserInput::Image {
        image_url: "data:image/png;base64,AA==".to_string(),
        detail: None,
    }];

    assert_eq!(
        image_fallback_model("gpt-5.3-codex-spark", &items),
        Some("gpt-5.6-luna")
    );
}

#[test]
fn spark_keeps_text_input() {
    let items = vec![UserInput::Text {
        text: "hello".to_string(),
        text_elements: Vec::new(),
    }];

    assert_eq!(image_fallback_model("gpt-5.3-codex-spark", &items), None);
}
