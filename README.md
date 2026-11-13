# transyt

A Rust YouTube transcript downloader and MCP server. Give an agent a YouTube
link; `get_transcript` returns the complete available caption track, timestamps,
language metadata, and full text. It does not watch videos, transcribe audio,
summarize, or silently shorten captions. No Python, browser, YouTube login, or
user-supplied API key is used.

**Implementation status:** initial implementation, not yet production-ready.
The native test suite reaches 100% measured first-party Rust line coverage,
including the CLI, with that threshold enforced in CI. Line coverage is not
branch coverage or proof of correctness. Worker runtime validation and published
versioned releases are still separate milestones.

## Run locally

Build with stable Rust:

```sh
cargo build --locked --release
./target/release/transyt fetch 'https://www.youtube.com/watch?v=VIDEO_ID_11' transcript.txt
```

The output file must not already exist. Without a filename, `fetch` prints JSON
with every timed snippet. The executable uses Rustls, not a separately installed
OpenSSL library. Windows uses `target/release/transyt.exe`.

### Test binaries from GitHub Actions

Open a successful run of the **CI** workflow in this repository's **Actions**
tab and download the artifact matching your OS and CPU architecture:
`transyt-Linux-X64`, `transyt-macOS-ARM64`, or `transyt-Windows-X64`
(the artifact name reflects the actual runner architecture).
Extract the zip; the executable and license notices are at its root.
On macOS/Linux, run `chmod +x transyt` after extraction because GitHub's artifact
zip does not preserve executable permissions. These are unsigned development
binaries, not published releases.

Register the extracted executable using its **absolute path** in the commands
below. You do not need Rust, Node.js, Python, or a package manager to run it.
Keep the included license notices if you redistribute a binary.

### Claude Code and Codex

After putting the executable on PATH:

```sh
claude mcp add --transport stdio transyt -- transyt stdio
codex mcp add transyt -- transyt stdio
```

Alternatively replace the executable name after `--` with its absolute path.
No environment variables or credentials are needed. Stdio is the default;
stdout contains only MCP messages.

The tool accepts `video` (URL or ID), optional ordered `languages` (e.g.
`["de","en"]`), and optional `translate` (e.g. `"fr"`). Without language preferences,
the first available track is used. With preferences, manual captions win over
generated captions within the requested language. Translation requires YouTube
to advertise support. Ask the agent to save the returned `text` using its own
filesystem tools: a remote server cannot write files onto the agent's computer.

### HTTP

```sh
transyt http
claude mcp add --transport http transyt-http http://127.0.0.1:8080/mcp
codex mcp add transyt-http --url http://127.0.0.1:8080/mcp
```

The stateless `/mcp` endpoint returns JSON responses over Streamable HTTP, or SSE
message events when the client accepts only `text/event-stream`.
It does not implement the deprecated separate `/sse` + message-post transport
or server-initiated notifications.
Native HTTP defaults to loopback and rejects browser Origin headers. Do not
expose the unauthenticated native listener directly to the internet.

## Cloudflare Worker

Rust compiles to WebAssembly because Workers do not execute ordinary native
binaries. Application logic is shared with the native server; Workers tooling
generates the small JavaScript runtime shim. There is no Python runtime.

Prerequisites: Rust/rustup, Node.js/npm, and a Cloudflare account.

```sh
rustup target add wasm32-unknown-unknown
npx wrangler login
npx wrangler deploy
```

Wrangler uses `wrangler.toml` to build the Worker. Connect a remote MCP client to
`https://transyt.<your-subdomain>.workers.dev/mcp`. For Claude/Codex use the HTTP
commands above with that URL. ChatGPT remote connector availability depends on
your account and client; this repository does not provision a Cloudflare gateway
or configure ChatGPT for you.

The Worker endpoint has no application authentication. Anyone with the URL can
use it and consume your quota. Configure Cloudflare rate limits and budget
controls before public sharing. Cloudflare account login is deployment
authentication, not a YouTube credential.

**Hosting limitation:** upstream explicitly warns that YouTube blocks many cloud
provider IPs. A successful deployment does not prove transcript retrieval works
from Cloudflare. Blocked, age-restricted, private, consent-gated, token-required,
or captionless videos fail explicitly. No credential or proxy bypass is provided.
YouTube's undocumented endpoints can change without notice.

## Development and checks

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo check --locked --lib --target wasm32-unknown-unknown
cargo llvm-cov --locked --all-targets --fail-under-lines 100
cargo deny check licenses sources
cargo about generate --locked --fail about.hbs --output-file DEPENDENCY_LICENSES.md
```

Install `cargo-llvm-cov`, the `llvm-tools-preview` rustup component,
`cargo-deny`, and `cargo-about` (with its `cli` feature) for the optional tools.
Offline fixture tests do not assert live
YouTube availability. CI builds/tests Linux, macOS, and Windows and checks the
Worker target. Native coverage is not a measurement of the Worker runtime.

## License and credits

MIT; see [LICENSE](LICENSE) and [third-party notices](THIRD_PARTY_NOTICES.md).
Transcript discovery is adapted from Jonas Depoix's
[youtube-transcript-api](https://github.com/jdepoix/youtube-transcript-api).
No upstream Python source is shipped.

Direct library credits:

- [Serde](https://github.com/serde-rs/serde) and
  [serde_json](https://github.com/serde-rs/json): typed JSON.
- [rust-url](https://github.com/servo/rust-url): URL validation.
- [regex](https://github.com/rust-lang/regex): caption cleanup and key extraction.
- [roxmltree](https://github.com/RazrFalcon/roxmltree): XML parsing.
- [html-escape](https://github.com/magiclen/html-escape): caption entity decoding.
- [reqwest](https://github.com/seanmonstar/reqwest) and
  [Rustls](https://github.com/rustls/rustls): native HTTPS.
- [Tokio and Axum](https://github.com/tokio-rs): native I/O and HTTP serving.
- [workers-rs](https://github.com/cloudflare/workers-rs): Cloudflare runtime adapter.
- [async-trait](https://github.com/dtolnay/async-trait): a shared, object-safe
  network interface for production adapters and test fixtures.

`Cargo.lock` pins transitive dependencies; `cargo-deny` enforces the permitted
MIT-compatible license policy. This project being MIT does not relicense its
dependencies. CI generates `DEPENDENCY_LICENSES.md` with cargo-about and bundles
it alongside native artifacts and the upstream notice. Keep all notices with
any binaries you redistribute. Published, versioned release archives are not
yet configured.
