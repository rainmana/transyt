//! Exercise the real native adapters against local sockets and child-process stdio.
use serde_json::json;
use std::{
    io::Write,
    process::{Command, Stdio},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use transyt::Http;

#[test]
fn stdio_protocol_and_usage() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_transyt"))
        .arg("stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        writeln!(input, "not-json").unwrap();
        writeln!(
            input,
            "{}",
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .unwrap();
        for method in ["initialize", "tools/list", "ping"] {
            writeln!(input, "{}", json!({"jsonrpc":"2.0","id":1,"method":method})).unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let messages: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0]["error"]["code"], -32700);
    assert_eq!(messages[2]["result"]["tools"][0]["name"], "get_transcript");
    for args in [
        vec!["unknown"],
        vec!["fetch"],
        vec!["fetch", "invalid"],
        vec!["http", "not-an-address"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_transyt"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
}

/// Serve one controlled HTTP response and inspect the request on the wire.
async fn server(response: &'static str) -> (String, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 8192];
        let count = stream.read(&mut request).await.unwrap();
        stream.write_all(response.as_bytes()).await.unwrap();
        String::from_utf8(request[..count].to_vec()).unwrap()
    });
    (format!("http://{address}/test"), task)
}

#[tokio::test]
async fn native_get_post_status_redirect_and_network_errors() {
    let client = transyt::native::Client::new().unwrap();
    for body in [None, Some(json!({"hello":"world"}))] {
        let (url, task) =
            server("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok").await;
        let is_post = body.is_some();
        assert_eq!(client.request(&url, body).await.unwrap(), "ok");
        let request = task.await.unwrap();
        assert!(request.starts_with(if is_post { "POST " } else { "GET " }));
    }
    for response in [
        "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        "HTTP/1.1 302 Found\r\nLocation: http://localhost/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort",
    ] {
        let (url, task) = server(response).await;
        assert!(client.request(&url, None).await.is_err());
        task.await.unwrap();
    }
    assert!(client.request("not a URL", None).await.is_err());
}
