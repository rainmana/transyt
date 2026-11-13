//! Worker adapter; only this module depends on the JavaScript-hosted Workers runtime.
use crate::Http;
use serde_json::Value;
use worker::*;

struct Client;

#[async_trait::async_trait(?Send)]
impl Http for Client {
    async fn request(&self, url: &str, body: Option<Value>) -> std::result::Result<String, String> {
        let mut init = RequestInit::new();
        init.with_redirect(RequestRedirect::Manual);
        if let Some(body) = body {
            init.with_method(Method::Post)
                .with_body(Some(body.to_string().into()));
            let headers = Headers::new();
            headers
                .set("content-type", "application/json")
                .map_err(|e| e.to_string())?;
            init.with_headers(headers);
        }
        let request = Request::new_with_init(url, &init).map_err(|e| e.to_string())?;
        let mut response = Fetch::Request(request)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !(200..300).contains(&response.status_code()) {
            return Err(format!(
                "YouTube HTTP {}; cloud IP blocking or consent may prevent access",
                response.status_code()
            ));
        }
        response.text().await.map_err(|e| e.to_string())
    }
}

#[event(fetch)]
async fn main(mut request: Request, _env: Env, _ctx: Context) -> Result<Response> {
    if request.path() != "/mcp" {
        return Response::error("Not found", 404);
    }
    if request.method() != Method::Post {
        return Response::error("Use MCP POST", 405);
    }
    let message = match request.json::<Value>().await {
        Ok(value) => value,
        Err(_) => return Response::error("Invalid JSON", 400),
    };
    match crate::dispatch(&Client, message).await {
        Some(value) => {
            let sse = request.headers().get("accept")?.is_some_and(|value| {
                value.contains("text/event-stream") && !value.contains("application/json")
            });
            let (content_type, body) = crate::http_body(&value, sse);
            let headers = Headers::new();
            headers.set("content-type", content_type)?;
            headers.set("cache-control", "no-store")?;
            Ok(Response::ok(body)?.with_headers(headers))
        }
        None => Ok(Response::empty()?.with_status(202)),
    }
}
