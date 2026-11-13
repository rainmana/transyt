//! Offline fixtures exercise retrieval without relying on YouTube availability.
use serde_json::{Value, json};
use std::sync::Mutex;
use transyt::{Http, fetch, parse_captions, video_id};

/// Test messages default to valid JSON-RPC envelopes.
async fn dispatch(http: &impl Http, mut message: Value) -> Option<Value> {
    message["jsonrpc"] = json!("2.0");
    transyt::dispatch(http, message).await
}

struct Fixture(Mutex<Vec<Result<String, String>>>);
#[async_trait::async_trait]
impl Http for Fixture {
    async fn request(&self, url: &str, _: Option<Value>) -> Result<String, String> {
        assert!(url.starts_with("https://www.youtube.com/"));
        self.0.lock().unwrap().remove(0)
    }
}

fn fixture(player: Value, xml: &str) -> Fixture {
    Fixture(Mutex::new(vec![
        Ok(r#"{"INNERTUBE_API_KEY":"public_key"}"#.into()),
        Ok(player.to_string()),
        Ok(xml.into()),
    ]))
}

fn player() -> Value {
    json!({"playabilityStatus":{"status":"OK"},"captions":{"playerCaptionsTracklistRenderer":{
        "captionTracks":[
            {"baseUrl":"https://www.youtube.com/api/timedtext?v=abcdefghijk&fmt=srv3","languageCode":"en","kind":"asr"},
            {"baseUrl":"https://www.youtube.com/api/timedtext?v=abcdefghijk","languageCode":"en","isTranslatable":true}
        ],"translationLanguages":[{"languageCode":"de"}]
    }}})
}

#[tokio::test]
async fn protocol_validation_sse_and_successful_tool_call() {
    let http = Fixture(Mutex::new(vec![]));
    for invalid in [
        json!(null),
        json!([]),
        json!({"id":true,"method":"ping","jsonrpc":"2.0"}),
        json!({"id":1,"method":"ping"}),
    ] {
        assert_eq!(
            transyt::dispatch(&http, invalid).await.unwrap()["error"]["code"],
            -32600
        );
    }
    for protocol in ["2024-11-05", "2025-06-18", "2025-11-25", "2025-03-26"] {
        let response = dispatch(
            &http,
            json!({"id":1,"method":"initialize","params":{"protocolVersion":protocol}}),
        )
        .await
        .unwrap();
        assert_eq!(response["result"]["protocolVersion"], protocol);
    }
    for arguments in [
        json!({"video":"abcdefghijk","translate":false}),
        json!({"video":"abcdefghijk","extra":true}),
    ] {
        assert_eq!(dispatch(&http, json!({"id":1,"method":"tools/call","params":{"name":"get_transcript","arguments":arguments}})).await.unwrap()["error"]["code"], -32602);
    }
    let response = dispatch(
        &fixture(player(), r#"<transcript><text start="0">complete</text></transcript>"#),
        json!({"id":1,"method":"tools/call","params":{"name":"get_transcript","arguments":{"video":"abcdefghijk"}}}),
    ).await.unwrap();
    assert_eq!(response["result"]["isError"], false);
    let (mime, body) = transyt::http_body(&response, true);
    assert_eq!(mime, "text/event-stream");
    assert!(body.starts_with("event: message\ndata: "));
    assert!(body.ends_with("\n\n"));
    let (mime, body) = transyt::http_body(&response, false);
    assert_eq!(mime, "application/json");
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), response);
    let mut token_player = player();
    token_player["captions"]["playerCaptionsTracklistRenderer"]["captionTracks"][0]["baseUrl"] =
        json!("https://www.youtube.com/api/timedtext?exp=xpe");
    assert!(
        fetch(&fixture(token_player, ""), "abcdefghijk", &[], None)
            .await
            .unwrap_err()
            .contains("token")
    );
}

#[test]
fn links_and_untrusted_hosts() {
    for link in [
        "abcdefghijk",
        "https://youtu.be/abcdefghijk?t=2",
        "https://www.youtube.com/watch?v=abcdefghijk&feature=share",
        "https://youtube.com/shorts/abcdefghijk",
        "https://m.youtube.com/embed/abcdefghijk",
        "https://youtube.com/live/abcdefghijk",
    ] {
        assert_eq!(video_id(link).unwrap(), "abcdefghijk");
    }
    for bad in [
        "",
        "short",
        "https://youtube.com.evil.test/watch?v=abcdefghijk",
        "https://evil.test/abcdefghijk",
        "file:///abcdefghijk",
        "https://youtube.com/playlist?list=abcdefghijk",
        "https://youtu.be/short",
    ] {
        assert!(video_id(bad).is_err(), "{bad}");
    }
}

#[test]
fn caption_decoding_and_invalid_data() {
    let snippets = parse_captions(r#"<transcript><text start="1.2" dur="2">Hi &amp;amp; &lt;b&gt;you&lt;/b&gt;</text><text start="4">last</text></transcript>"#).unwrap();
    assert_eq!(snippets.len(), 2);
    assert_eq!(snippets[0].text, "Hi & you");
    assert_eq!(snippets[1].duration, 0.0);
    for bad in [
        "broken",
        "<wrong/>",
        "<transcript><text/></transcript>",
        r#"<transcript><text start="NaN"/></transcript>"#,
        r#"<transcript><text start="-1"/></transcript>"#,
    ] {
        assert!(parse_captions(bad).is_err());
    }
}

#[tokio::test]
async fn complete_transcript_and_translation() {
    let xml = r#"<transcript><text start="0">first</text><text start="10" dur="3">last</text></transcript>"#;
    let result = fetch(&fixture(player(), xml), "abcdefghijk", &["en".into()], None)
        .await
        .unwrap();
    assert!(!result.is_generated);
    assert_eq!(result.text, "first\nlast");
    assert_eq!(result.snippets.len(), 2);
    let result = fetch(
        &fixture(player(), xml),
        "abcdefghijk",
        &["en".into()],
        Some("de"),
    )
    .await
    .unwrap();
    assert_eq!(result.language_code, "de");
    assert!(result.is_generated);
}

#[tokio::test]
async fn retrieval_errors_are_explicit() {
    let mut blocked = player();
    blocked["playabilityStatus"] = json!({"status":"LOGIN_REQUIRED","reason":"blocked"});
    assert!(
        fetch(&fixture(blocked, ""), "abcdefghijk", &[], None)
            .await
            .unwrap_err()
            .contains("blocked")
    );
    for p in [json!({}), player()] {
        assert!(
            fetch(&fixture(p, ""), "abcdefghijk", &["fr".into()], None)
                .await
                .is_err()
        );
    }
    assert!(
        fetch(
            &fixture(player(), ""),
            "abcdefghijk",
            &["en".into()],
            Some("fr")
        )
        .await
        .is_err()
    );
    let mut unsafe_player = player();
    unsafe_player["captions"]["playerCaptionsTracklistRenderer"]["captionTracks"][0]["baseUrl"] =
        json!("https://localhost/api/timedtext");
    assert!(
        fetch(&fixture(unsafe_player, ""), "abcdefghijk", &[], None)
            .await
            .is_err()
    );
    assert!(
        fetch(
            &fixture(player(), "<transcript/>"),
            "abcdefghijk",
            &[],
            None
        )
        .await
        .is_err()
    );
    let network = Fixture(Mutex::new(vec![Err("offline".into())]));
    assert_eq!(
        fetch(&network, "abcdefghijk", &[], None).await.unwrap_err(),
        "offline"
    );
}

#[tokio::test]
async fn malformed_caption_metadata_never_reaches_the_network() {
    for (url, expected) in [
        (Value::Null, "Missing caption URL"),
        (json!("not a URL"), "Invalid caption URL"),
        (
            json!("http://www.youtube.com/api/timedtext"),
            "Unexpected caption URL destination",
        ),
        (
            json!("https://www.youtube.com/other"),
            "Unexpected caption URL destination",
        ),
        (
            json!("https://www.youtube.com:8443/api/timedtext"),
            "Unexpected caption URL destination",
        ),
        (
            json!("https://user@www.youtube.com/api/timedtext"),
            "Unexpected caption URL destination",
        ),
        (
            json!("https://:password@www.youtube.com/api/timedtext"),
            "Unexpected caption URL destination",
        ),
    ] {
        let mut metadata = player();
        metadata["captions"]["playerCaptionsTracklistRenderer"]["captionTracks"][0]["baseUrl"] =
            url;
        let http = fixture(metadata, "must not be requested");
        assert_eq!(
            fetch(&http, "abcdefghijk", &[], None).await.unwrap_err(),
            expected
        );
        assert_eq!(
            http.0.lock().unwrap().len(),
            1,
            "caption request must not be sent"
        );
    }
}

#[tokio::test]
async fn mcp_discovery_and_errors() {
    let http = Fixture(Mutex::new(vec![]));
    for method in ["initialize", "ping", "tools/list"] {
        let response = dispatch(&http, json!({"jsonrpc":"2.0","id":1,"method":method}))
            .await
            .unwrap();
        assert!(response.get("result").is_some());
        assert_eq!(response["id"], 1);
    }
    assert!(
        dispatch(&http, json!({"method":"notifications/initialized"}))
            .await
            .is_none()
    );
    for request in [
        json!({"id":2,"method":"unknown"}),
        json!({"id":2,"method":"tools/call","params":{"name":"other"}}),
        json!({"id":2,"method":"tools/call","params":{"name":"get_transcript","arguments":{}}}),
        json!({"id":2,"method":"tools/call","params":{"name":"get_transcript","arguments":{"video":"abcdefghijk","languages":3}}}),
    ] {
        assert!(
            dispatch(&http, request)
                .await
                .unwrap()
                .get("error")
                .is_some()
        );
    }
    let result = dispatch(&http, json!({"id":3,"method":"tools/call","params":{"name":"get_transcript","arguments":{"video":"invalid"}}})).await.unwrap();
    assert_eq!(result["result"]["isError"], true);
}
