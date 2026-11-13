//! Test the public CLI and HTTP service against fixtures and real loopback sockets.
use serde_json::{Value, json};
use transyt::cli::{run, run_until_shutdown, serve_http};

struct Captions;
#[async_trait::async_trait]
impl transyt::Http for Captions {
    async fn request(&self, url: &str, _: Option<Value>) -> Result<String, String> {
        Ok(if url.contains("/watch?") {
            r#"{"INNERTUBE_API_KEY":"key"}"#
        } else if url.contains("/player?") {
            r#"{"captions":{"playerCaptionsTracklistRenderer":{"captionTracks":[{"languageCode":"en","baseUrl":"https://www.youtube.com/api/timedtext"}]}}}"#
        } else {
            r#"<transcript><text start="0">entire transcript</text></transcript>"#
        }.into())
    }
}

#[tokio::test]
async fn file_download_and_safe_overwrite_behavior() {
    let path = std::env::temp_dir().join(format!("transyt-test-{}.txt", std::process::id()));
    let args = vec![
        "fetch".into(),
        "abcdefghijk".into(),
        path.to_str().unwrap().into(),
    ];
    run(&Captions, &args).await.unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "entire transcript");
    assert!(run(&Captions, &args).await.is_err());
    std::fs::remove_file(&path).unwrap();
    run(&Captions, &args[..2]).await.unwrap();
    assert!(
        run(&Captions, &["http".into(), "not-an-address".into()])
            .await
            .is_err()
    );
    run_until_shutdown(
        &Captions,
        &["http".into(), "127.0.0.1:0".into()],
        Box::pin(std::future::ready(())),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn http_json_sse_notifications_and_origin_checks() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        serve_http(listener, async {
            let _ = stopped.await;
        })
        .await
        .unwrap();
    });
    let client = reqwest::Client::new();
    for accept in ["application/json", "text/event-stream"] {
        let response = client
            .post(&url)
            .header("accept", accept)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"ping"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.headers()["content-type"], accept);
        let body = response.text().await.unwrap();
        assert!(body.contains("\"result\":{}"));
    }
    let response = client
        .post(&url)
        .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 202);
    let response = client
        .post(&url)
        .header("origin", "https://untrusted.example")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"ping"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    stop.send(()).unwrap();
    server.await.unwrap();
}
