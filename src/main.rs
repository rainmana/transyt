//! Thin native entry point; command behavior lives in the testable library.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let client = transyt::native::Client::new()?;
    transyt::cli::run(&client, &args).await
}
