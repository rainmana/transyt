//! Native commands: newline-delimited MCP stdio, HTTP, or direct transcript download.
use crate as transyt;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

/// Keep command behavior injectable so file downloads can be tested without YouTube.
pub async fn run(
    client: &dyn transyt::Http,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    run_until_shutdown(client, args, Box::pin(std::future::pending())).await
}

/// Run native commands with an HTTP shutdown signal supplied by the embedding application.
pub async fn run_until_shutdown(
    client: &dyn transyt::Http,
    args: &[String],
    shutdown: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.first().map(String::as_str) {
        Some("fetch") => {
            let video = args.get(1).ok_or("Usage: transyt fetch URL [OUTPUT.txt]")?;
            let transcript = transyt::fetch(client, video, &[], None).await?;
            write_transcript(&transcript, args.get(2).map(String::as_str))?;
        }
        Some("http") => {
            let address = args.get(1).map(String::as_str).unwrap_or("127.0.0.1:8080");
            eprintln!("MCP listening on http://{address}/mcp");
            serve_http(tokio::net::TcpListener::bind(address).await?, shutdown).await?;
        }
        None | Some("stdio") => {
            let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
            let mut stdout = tokio::io::stdout();
            while let Some(line) = lines.next_line().await? {
                let response = match serde_json::from_str(&line) {
                    Ok(request) => transyt::dispatch(client, request).await,
                    Err(_) => Some(
                        json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
                    ),
                };
                if let Some(response) = response {
                    stdout.write_all(format!("{response}\n").as_bytes()).await?;
                    stdout.flush().await?;
                }
            }
        }
        _ => {
            return Err(
                "Usage: transyt [stdio | http [127.0.0.1:8080] | fetch URL [OUTPUT.txt]]".into(),
            );
        }
    }
    Ok(())
}

/// Keep output handling independent of the network adapter that retrieved the captions.
fn write_transcript(
    transcript: &transyt::Transcript,
    path: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = path {
        // Create-new avoids accidentally overwriting an existing user file.
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(transcript.text.as_bytes())?;
    } else {
        println!("{}", serde_json::to_string_pretty(transcript)?);
    }
    Ok(())
}

/// Run the same HTTP service in production and in socket-level tests.
/// The shutdown future lets an embedding application drain in-flight requests.
pub async fn serve_http(
    listener: tokio::net::TcpListener,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, router())
        .with_graceful_shutdown(shutdown)
        .await
}

/// Construct the native endpoint separately from socket lifetime and CLI parsing.
fn router() -> axum::Router {
    use axum::{Router, routing::post};
    Router::new().route(
        "/mcp",
        post(
            |headers: axum::http::HeaderMap, axum::Json(request): axum::Json<Value>| async move {
                // No browser-origin traffic on the unauthenticated native endpoint.
                if headers.contains_key("origin") {
                    return axum::response::Response::builder()
                        .status(403)
                        .body(axum::body::Body::empty())
                        .unwrap();
                }
                let client = transyt::native::Client::new().unwrap();
                match transyt::dispatch(&client, request).await {
                    Some(value) => {
                        let sse = headers
                            .get("accept")
                            .and_then(|v| v.to_str().ok())
                            .is_some_and(|v| {
                                v.contains("text/event-stream") && !v.contains("application/json")
                            });
                        let (content_type, body) = transyt::http_body(&value, sse);
                        axum::response::Response::builder()
                            .header("content-type", content_type)
                            .header("cache-control", "no-store")
                            .body(axum::body::Body::from(body))
                            .unwrap()
                    }
                    None => axum::response::Response::builder()
                        .status(202)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                }
            },
        ),
    )
}
