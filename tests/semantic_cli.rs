use std::fs;
use std::io::{self, BufRead, BufReader, Read, Seek, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::{TempDir, tempdir, tempfile_in};

const MODEL: &str = "synthetic-model-v1";
const TOKEN: &str = "synthetic-secret-not-a-credential";
const BODY_CANARY: &str = "synthetic-provider-body-must-not-leak";
const STORE: &str = ".kb-embed.json";
const LIMIT: Duration = Duration::from_secs(15);

fn command(root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_knowbox"));
    cmd.env_clear()
        .current_dir(root)
        .env("HOME", root)
        .env("USERPROFILE", root)
        .env("KNOWBOX_ROOT", root)
        .env("KNOWBOX_EMBED_MODEL", MODEL)
        .env("KNOWBOX_NO_RECENCY", "1")
        .args(args)
        .stdin(Stdio::null());
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        cmd.env("SystemRoot", system_root);
    }
    cmd
}

fn sandbox() -> io::Result<TempDir> {
    let root = tempdir()?;
    fs::create_dir(root.path().join("notes"))?;
    Ok(root)
}

fn note(root: &Path, id: &str, title: &str, summary: &str, status: &str) -> io::Result<()> {
    fs::write(
        root.join("notes").join(format!("{id}-fixture.md")),
        format!(
            "---\nid: {id}\ntitle: {title}\ntype: fact\ndomain: synthetic\nstatus: {status}\nconfidence: high\ncreated: 2026-01-01\nupdated: 2026-01-02\nsummary: {summary}\nrelated: []\nsupersedes: []\n---\nBody.\n"
        ),
    )
}

fn ok(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn ids(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|line| line.starts_with("kb-"))
        .filter_map(|line| line.split_whitespace().next())
        .collect()
}

fn vectors(pairs: &[(usize, [f64; 2])]) -> Value {
    json!({"data": pairs.iter().map(|(i, v)| json!({"index": i, "embedding": v})).collect::<Vec<_>>()})
}

fn receive(stream: &mut TcpStream) -> io::Result<Value> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut reader = BufReader::new(stream).take(32 * 1024);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if line != "POST /embeddings HTTP/1.1\r\n" {
        return Err(io::Error::other("unexpected mock request method or path"));
    }
    let mut size = None;
    let mut authorized = false;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete mock request headers",
            ));
        }
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                size = Some(value.trim().parse::<u16>().map_err(io::Error::other)?);
            } else if name.eq_ignore_ascii_case("authorization") {
                authorized = value.trim() == format!("Bearer {TOKEN}");
            }
        }
    }
    if !authorized {
        return Err(io::Error::other(
            "request did not carry the synthetic bearer token",
        ));
    }
    let size = size.ok_or_else(|| io::Error::other("missing request Content-Length"))?;
    let mut body = vec![0; usize::from(size)];
    reader.read_exact(&mut body)?;
    let mut payload: Value = serde_json::from_slice(&body)?;
    if payload["model"] != MODEL {
        return Err(io::Error::other("unexpected mock request model"));
    }
    Ok(payload["input"].take())
}

fn run(
    root: &Path,
    cmd: &mut Command,
    replies: &[(u16, Value)],
) -> io::Result<(Output, Vec<Value>)> {
    run_before_reply(root, cmd, replies, |_| Ok(()))
}

fn run_before_reply(
    root: &Path,
    cmd: &mut Command,
    replies: &[(u16, Value)],
    mut before_reply: impl FnMut(usize) -> io::Result<()>,
) -> io::Result<(Output, Vec<Value>)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    cmd.env(
        "KNOWBOX_EMBED_URL",
        format!("http://{}/embeddings", listener.local_addr()?),
    );
    let mut inputs = Vec::new();
    let output = run_with_server(root, cmd, listener, |stream| {
        let input = receive(stream)?;
        before_reply(inputs.len())?;
        let (code, body) = replies.get(inputs.len()).map_or_else(
            || (500, "{}".into()),
            |(code, body)| (*code, body.to_string()),
        );
        inputs.push(input);
        write!(
            stream,
            "HTTP/1.1 {code} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    })?;
    Ok((output, inputs))
}

fn run_with_server(
    root: &Path,
    cmd: &mut Command,
    listener: TcpListener,
    mut exchange: impl FnMut(&mut TcpStream) -> io::Result<()>,
) -> io::Result<Output> {
    listener.set_nonblocking(true)?;
    let mut stdout = tempfile_in(root)?;
    let mut stderr = tempfile_in(root)?;
    let mut child = cmd
        .env("KNOWBOX_API_KEY", TOKEN)
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .spawn()?;
    let deadline = Instant::now() + LIMIT;
    let status = loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // An accepted socket can inherit the listener's nonblocking mode.
                if let Err(error) = stream
                    .set_nonblocking(false)
                    .and_then(|()| exchange(&mut stream))
                {
                    child.kill()?;
                    child.wait()?;
                    return Err(error);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                thread::park_timeout(Duration::from_millis(5));
            }
            Err(error) => {
                child.kill()?;
                child.wait()?;
                return Err(error);
            }
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "semantic CLI deadline",
            ));
        }
    };
    stdout.rewind()?;
    stderr.rewind()?;
    let mut output = Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    stdout.read_to_end(&mut output.stdout)?;
    stderr.read_to_end(&mut output.stderr)?;
    Ok(output)
}

fn expect_tls_client_hello(stream: &mut TcpStream) -> io::Result<()> {
    let mut record = [0u8; 6];
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.read_exact(&mut record)?;
    if record[0] != 22 || record[1] != 3 || record[5] != 1 {
        return Err(io::Error::other("expected a TLS ClientHello"));
    }
    Ok(())
}

#[test]
fn https_proxy_uses_connect_then_tls_and_keeps_provider_auth_inside_the_tunnel() -> io::Result<()> {
    let tmp = sandbox()?;
    let root = tmp.path();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let mut cmd = command(root, &["embed"]);
    cmd.env(
        "KNOWBOX_EMBED_URL",
        "https://provider.invalid:8443/embeddings",
    )
    .env(
        "HTTPS_PROXY",
        format!("http://proxy:proxy-pass@{}", listener.local_addr()?),
    )
    .env("ALL_PROXY", "http://127.0.0.1:1");
    let mut connections = 0;
    let output = run_with_server(root, &mut cmd, listener, |stream| {
        connections += 1;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let mut headers = String::new();
        let mut reader = BufReader::new((&mut *stream).take(8192));
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                return Err(io::Error::other("incomplete CONNECT request"));
            }
            if line == "\r\n" {
                break;
            }
            headers.push_str(&line);
        }
        assert_eq!(
            headers.lines().next(),
            Some("CONNECT provider.invalid:8443 HTTP/1.1")
        );
        assert!(
            headers
                .lines()
                .any(|line| line == "Proxy-Authorization: Basic cHJveHk6cHJveHktcGFzcw==")
        );
        assert!(!headers.to_ascii_lowercase().contains("\r\nauthorization:"));
        assert!(!headers.contains(TOKEN));
        stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
        expect_tls_client_hello(stream)
    })?;
    assert_eq!(connections, 1);
    assert_eq!(output.status.code(), Some(1));
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostics.contains("embedding request failed"));
    for private in ["provider.invalid", "proxy-pass", TOKEN] {
        assert!(!diagnostics.contains(private));
    }
    Ok(())
}

#[test]
fn no_proxy_bypasses_connect_for_a_matching_https_host() -> io::Result<()> {
    let tmp = sandbox()?;
    let root = tmp.path();
    for bypass in ["example.invalid, 127.0.0.1 ", "*"] {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let address = listener.local_addr()?;
        let mut cmd = command(root, &["embed"]);
        let proxy = if bypass == "*" {
            "socks://ignored".to_owned()
        } else {
            format!("http://{address}")
        };
        cmd.env("KNOWBOX_EMBED_URL", format!("https://{address}/embeddings"))
            .env("https_proxy", proxy)
            .env("NO_PROXY", bypass);
        let mut connections = 0;
        let output = run_with_server(root, &mut cmd, listener, |stream| {
            connections += 1;
            expect_tls_client_hello(stream)
        })?;
        assert_eq!(connections, 1);
        assert_eq!(output.status.code(), Some(1));
    }
    Ok(())
}

#[test]
fn invalid_https_proxy_is_reported_without_echoing_its_value() -> io::Result<()> {
    let tmp = sandbox()?;
    let output = command(tmp.path(), &["embed"])
        .env("KNOWBOX_EMBED_URL", "https://127.0.0.1:1/embeddings")
        .env("KNOWBOX_API_KEY", TOKEN)
        .env("HTTPS_PROXY", "socks5://proxy:proxy-pass@127.0.0.1:1")
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "knowbox: invalid embedding proxy configuration (check HTTPS_PROXY and NO_PROXY)\n"
    );
    Ok(())
}

#[test]
fn embed_releases_writer_lock_during_http_and_skips_concurrently_retired_note() -> anyhow::Result<()>
{
    let tmp = sandbox()?;
    let root = tmp.path();
    note(root, "kb-0001", "Alpine", "First summary", "active")?;
    note(root, "kb-0002", "Birch", "Second summary", "active")?;

    let (embedded, inputs) = run_before_reply(
        root,
        &mut command(root, &["embed"]),
        &[
            (200, vectors(&[(0, [1., 0.])])),
            (200, vectors(&[(0, [1., 0.]), (1, [0., 1.])])),
        ],
        |request| {
            if request == 1 {
                // Embed has read both notes and cannot finish HTTP until this writer exits.
                let superseded = command(root, &["supersede", "kb-0001", "--with", "kb-0002"])
                    .env("KNOWBOX_LOCK_TIMEOUT", "0.25")
                    .output()?;
                if superseded.status.code() != Some(0) {
                    return Err(io::Error::other(format!(
                        "supersede failed while embed awaited HTTP: {}",
                        String::from_utf8_lossy(&superseded.stderr)
                    )));
                }
            }
            Ok(())
        },
    )?;
    ok(&embedded);
    assert_eq!(
        inputs,
        [
            json!(["warmup"]),
            json!([
                "Alpine\nFirst summary\nBody.\n",
                "Birch\nSecond summary\nBody.\n"
            ]),
        ]
    );
    assert_eq!(
        String::from_utf8(embedded.stdout)?,
        format!(
            "embedded 1 note(s); 1 total (model={MODEL}); 1 changed during refresh — rerun `knowbox embed`\n"
        )
    );
    let store: Value = serde_json::from_slice(&fs::read(root.join(STORE))?)?;
    let stored = store["notes"]
        .as_object()
        .ok_or_else(|| io::Error::other("missing cached notes"))?;
    assert_eq!(
        stored.keys().map(String::as_str).collect::<Vec<_>>(),
        ["kb-0002"]
    );
    assert_eq!(stored["kb-0002"]["v"], json!([0., 1.]));
    Ok(())
}

#[test]
fn provider_status_is_reported_without_body_or_auth() -> anyhow::Result<()> {
    let root = sandbox()?;
    let (output, inputs) = run(
        root.path(),
        command(root.path(), &["embed"]).env("HTTPS_PROXY", "invalid proxy for loopback"),
        &[(401, json!({"error": format!("{BODY_CANARY} {TOKEN}")}))],
    )?;
    assert_eq!(inputs, [json!(["warmup"])]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    let diagnostic = std::str::from_utf8(&output.stderr)?;
    assert!(diagnostic.contains("HTTP 401"), "{diagnostic}");
    assert!(!diagnostic.contains(BODY_CANARY));
    assert!(!diagnostic.contains(TOKEN));
    assert!(!root.path().join(STORE).exists());
    Ok(())
}

#[test]
fn embed_reorders_indexed_results_reuses_hashes_and_preserves_store_on_short_batch()
-> anyhow::Result<()> {
    let tmp = sandbox()?;
    let root = tmp.path();
    note(root, "kb-0001", "Alpine", "First summary", "active")?;
    note(root, "kb-0002", "Birch", "Second summary", "active")?;
    let warmup = (200, vectors(&[(0, [1., 0.])]));
    let expected_batch = json!([
        "Alpine\nFirst summary\nBody.\n",
        "Birch\nSecond summary\nBody.\n"
    ]);
    let (initial, input) = run(
        root,
        &mut command(root, &["embed"]),
        &[
            warmup.clone(),
            (200, vectors(&[(1, [0., 1.]), (0, [1., 0.])])),
        ],
    )?;
    ok(&initial);
    assert_eq!(input, [json!(["warmup"]), expected_batch.clone()]);
    let old = fs::read(root.join(STORE))?;
    let store: Value = serde_json::from_slice(&old)?;
    assert_eq!(store["model"], MODEL);
    assert_eq!(store["notes"]["kb-0001"]["v"], json!([1., 0.]));
    assert_eq!(store["notes"]["kb-0002"]["v"], json!([0., 1.]));
    for entry in ["kb-0001", "kb-0002"] {
        let hash = store["notes"][entry]["h"].as_str().unwrap_or("");
        assert_eq!(hash.len(), 40);
        assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
    let (incremental, input) = run(
        root,
        &mut command(root, &["embed"]),
        std::slice::from_ref(&warmup),
    )?;
    ok(&incremental);
    assert_eq!(input, [json!(["warmup"])]);
    assert_eq!(fs::read(root.join(STORE))?, old);

    let (failed, input) = run(
        root,
        &mut command(root, &["embed", "--force"]),
        &[
            warmup,
            (
                200,
                json!({"data": [{"index": 0, "embedding": [0., 1.]}], "detail": BODY_CANARY}),
            ),
        ],
    )?;
    assert_eq!(input, [json!(["warmup"]), expected_batch]);
    assert_eq!(failed.status.code(), Some(1));
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&failed.stdout),
        String::from_utf8_lossy(&failed.stderr)
    );
    assert!(!diagnostic.contains(TOKEN));
    assert!(!diagnostic.contains(BODY_CANARY));
    assert_eq!(fs::read(root.join(STORE))?, old);
    Ok(())
}

fn seed_store(root: &Path, entries: &[(&str, [f64; 2])]) -> io::Result<()> {
    let notes = entries
        .iter()
        .map(|(id, vector)| {
            (
                id.to_string(),
                json!({"h": "0000000000000000000000000000000000000000", "v": vector}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    fs::write(
        root.join(STORE),
        json!({"model": MODEL, "notes": notes}).to_string(),
    )
}

#[test]
fn exact_search_is_offline_and_partial_search_obeys_fusion_switch() -> anyhow::Result<()> {
    let tmp = sandbox()?;
    let root = tmp.path();
    note(root, "kb-0001", "Alpine", "First summary", "active")?;
    note(root, "kb-0002", "Birch", "Second summary", "active")?;
    ok(&command(root, &["reindex"]).output()?);
    seed_store(root, &[("kb-0001", [1., 0.]), ("kb-0002", [1., 0.])])?;

    let (exact, inputs) = run(root, &mut command(root, &["search", "alpine"]), &[])?;
    ok(&exact);
    assert_eq!(inputs, Vec::<Value>::new());
    assert_eq!(ids(std::str::from_utf8(&exact.stdout)?), ["kb-0001"]);

    let args = ["search", "alpine", "qzxwplnr"];
    let (disabled, inputs) = run(
        root,
        command(root, &args).env("KNOWBOX_NO_FUSION", "1"),
        &[],
    )?;
    ok(&disabled);
    assert_eq!(inputs, Vec::<Value>::new());
    let disabled = std::str::from_utf8(&disabled.stdout)?;
    assert_eq!(ids(disabled), ["kb-0001"]);
    assert_eq!(
        disabled.lines().next(),
        Some("(partial match — no note contains all query terms)")
    );

    let (enabled, inputs) = run(
        root,
        &mut command(root, &args),
        &[(200, vectors(&[(0, [1., 0.])]))],
    )?;
    ok(&enabled);
    assert_eq!(inputs, [json!(["alpine qzxwplnr"])]);
    let enabled = std::str::from_utf8(&enabled.stdout)?;
    assert_eq!(ids(enabled), ["kb-0001", "kb-0002"]);
    assert_eq!(
        enabled.lines().nth(1),
        Some("(hybrid re-rank: lexical + semantic agreement)")
    );
    assert!(
        enabled
            .lines()
            .any(|line| line == "kb-0002  notes/kb-0002-fixture.md (semantic-only)")
    );
    Ok(())
}

#[test]
fn lexical_miss_filters_at_point_four_but_explicit_vsearch_returns_lower_scores()
-> anyhow::Result<()> {
    let tmp = sandbox()?;
    let root = tmp.path();
    note(root, "kb-0001", "Alpine", "First summary", "active")?;
    note(root, "kb-0002", "Birch", "Second summary", "active")?;
    ok(&command(root, &["reindex"]).output()?);
    seed_store(root, &[("kb-0001", [2., 4.]), ("kb-0002", [2., 5.])])?;
    let replies = [(200, vectors(&[(0, [1., 0.])]))];
    let query = "qzxwplnr bltvjxqp";
    let (rescued, inputs) = run(root, &mut command(root, &["search", query]), &replies)?;
    ok(&rescued);
    assert_eq!(inputs, [json!([query])]);
    let rescued = std::str::from_utf8(&rescued.stdout)?;
    assert_eq!(ids(rescued), ["kb-0001"]);
    assert_eq!(
        rescued.lines().next(),
        Some(
            "(BM25 missed — semantic matches; cos <0.45 is likely-absent territory, verify before relying on these:)"
        )
    );
    assert_eq!(
        rescued.lines().nth(1),
        Some("kb-0001  notes/kb-0001-fixture.md  [cos 0.447]")
    );
    let usage = command(root, &["usage"]).output()?;
    ok(&usage);
    assert!(
        std::str::from_utf8(&usage.stdout)?
            .lines()
            .any(|line| line.ends_with("\tsemantic\t1"))
    );

    let (explicit, inputs) = run(
        root,
        &mut command(root, &["vsearch", "qzxwplnr", "bltvjxqp"]),
        &replies,
    )?;
    ok(&explicit);
    assert_eq!(inputs, [json!([query])]);
    let explicit = std::str::from_utf8(&explicit.stdout)?;
    assert_eq!(ids(explicit), ["kb-0001", "kb-0002"]);
    assert!(
        explicit
            .lines()
            .any(|line| line == "kb-0002  notes/kb-0002-fixture.md  [0.371]")
    );
    Ok(())
}

#[test]
fn conflicts_is_offline_and_json_is_deterministic_over_live_pairs() -> anyhow::Result<()> {
    let tmp = sandbox()?;
    let root = tmp.path();
    note(root, "kb-0001", "Alpine", "First summary", "active")?;
    note(root, "kb-0002", "Birch", "Deuxième", "draft")?;
    note(root, "kb-0003", "Cedar", "Third summary", "active")?;
    note(root, "kb-0004", "Retired", "Retired summary", "superseded")?;
    fs::write(
        root.join("notes/kb-0005-malformed.md"),
        "---\nid: kb-0005\n",
    )?;
    seed_store(
        root,
        &[
            ("kb-0001", [1., 0.]),
            ("kb-0002", [1., 0.]),
            ("kb-0003", [0., 1.]),
            ("kb-0004", [1., 0.]),
            ("kb-0005", [1., 0.]),
            ("kb-0006", [1., 0.]),
        ],
    )?;
    let args = ["conflicts", "--top", "3", "--min-cos", "0.9", "--json"];
    let (one, inputs) = run(root, &mut command(root, &args), &[])?;
    ok(&one);
    let (two, second_inputs) = run(root, &mut command(root, &args), &[])?;
    ok(&two);
    assert_eq!(inputs, Vec::<Value>::new());
    assert_eq!(second_inputs, Vec::<Value>::new());
    assert_eq!(one.stdout, two.stdout);
    let report: Value = serde_json::from_slice(&one.stdout)?;
    assert_eq!(
        report,
        json!([{
            "a": "kb-0001", "b": "kb-0002", "cos": 1.0,
            "a_status": "active", "b_status": "draft",
            "a_updated": "2026-01-02", "b_updated": "2026-01-02",
            "a_summary": "First summary", "b_summary": "Deuxième"
        }])
    );
    let judged = command(
        root,
        &[
            "verdict",
            "kb-0002",
            "kb-0001",
            "--verdict",
            "false-positive",
            "--reason",
            "same general term",
        ],
    )
    .output()?;
    ok(&judged);
    let (after_verdict, requests) = run(root, &mut command(root, &args), &[])?;
    ok(&after_verdict);
    assert!(requests.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&after_verdict.stdout)?,
        json!([])
    );
    Ok(())
}

#[test]
fn map_project_misses_sends_recent_misses_only_on_explicit_request() -> anyhow::Result<()> {
    let tmp = sandbox()?;
    let root = tmp.path();
    note(root, "kb-0001", "Alpine", "First summary", "active")?;
    note(root, "kb-0002", "Birch", "Second summary", "active")?;
    let (embedded, _) = run(
        root,
        &mut command(root, &["embed"]),
        &[
            (200, vectors(&[(0, [1., 0.])])),
            (200, vectors(&[(0, [1., 0.]), (1, [0., 1.])])),
        ],
    )?;
    ok(&embedded);
    let record = "2026-11-19T11:00:00\tmissing phrase\n";
    fs::write(root.join(".search-misses.log"), record)?;
    let cached = fs::read(root.join(STORE))?;
    let (offline, sent) = run(root, &mut command(root, &["map", "--json"]), &[])?;
    ok(&offline);
    assert!(sent.is_empty());
    let map: Value = serde_json::from_slice(&offline.stdout)?;
    assert_eq!(map["mode"], "embedding");
    assert_eq!(map["misses"]["state"], "offline");
    let (explicit, sent) = run(
        root,
        &mut command(root, &["map", "--json", "--project-misses"]),
        &[(200, vectors(&[(0, [10., 0.])]))],
    )?;
    ok(&explicit);
    assert_eq!(sent, [json!(["missing phrase"])]);
    let overlay: Value = serde_json::from_slice(&explicit.stdout)?;
    assert_eq!(overlay["nodes"], map["nodes"]);
    let placed = &overlay["misses"]["points"][0];
    assert_eq!(placed["query"], "missing phrase");
    assert_eq!(placed["x"], map["nodes"][0]["x"]);
    assert_eq!(placed["y"], map["nodes"][0]["y"]);
    assert_eq!(fs::read(root.join(STORE))?, cached);
    assert_eq!(fs::read_to_string(root.join(".search-misses.log"))?, record);
    Ok(())
}

#[test]
fn search_switches_control_correction_and_provider_calls_per_query() -> anyhow::Result<()> {
    let tmp = sandbox()?;
    let root = tmp.path();
    note(root, "kb-0001", "Alpine", "First summary", "active")?;
    note(root, "kb-0002", "Birch", "Second summary", "active")?;
    ok(&command(root, &["reindex"]).output()?);
    seed_store(root, &[("kb-0001", [1., 0.]), ("kb-0002", [1., 0.])])?;

    for (query, expected) in [
        ("alpine qzxwplnr", "partial"),
        ("qzxwplnr", "miss"),
        ("alpinez", "typo"),
    ] {
        let (result, requests) = run(
            root,
            &mut command(root, &["search", "--local", "--json", query]),
            &[],
        )?;
        ok(&result);
        assert_eq!(requests, Vec::<Value>::new());
        let value: Value = serde_json::from_slice(&result.stdout)?;
        assert_eq!(value["layer"], expected);
        if expected == "typo" {
            assert_eq!(
                value["corrections"],
                json!([{"from":"alpinez","to":"alpine"}])
            );
        }
    }
    let (result, requests) = run(
        root,
        &mut command(
            root,
            &["search", "--no-fix", "--local", "--json", "alpinez"],
        ),
        &[],
    )?;
    ok(&result);
    assert_eq!(requests, Vec::<Value>::new());
    let value: Value = serde_json::from_slice(&result.stdout)?;
    assert_eq!(value["layer"], "miss");
    assert_eq!(value["corrections"], json!([]));

    let query = "alpinez";
    let (result, requests) = run(
        root,
        &mut command(root, &["search", "--no-fix", "--json", query]),
        &[(200, vectors(&[(0, [1., 0.])]))],
    )?;
    ok(&result);
    assert_eq!(requests, [json!([query])]);
    let value: Value = serde_json::from_slice(&result.stdout)?;
    assert_eq!(value["layer"], "semantic");
    assert_eq!(value["hits"][0]["match_kind"], "semantic_only");
    assert_eq!(value["hits"][0]["cosine"], 1.0);
    assert_eq!(value["hits"][0]["cosine_rank"], 1);
    assert_eq!(value["hits"][0]["bm25_rank"], Value::Null);
    assert_eq!(value["hits"][0]["fused_rank"], Value::Null);
    Ok(())
}

#[test]
fn search_json_preserves_independent_lexical_cosine_and_fused_ranks() -> anyhow::Result<()> {
    let tmp = sandbox()?;
    let root = tmp.path();
    note(root, "kb-0001", "Alpine", "First summary", "active")?;
    note(root, "kb-0002", "Birch", "Second summary", "active")?;
    ok(&command(root, &["reindex"]).output()?);
    seed_store(root, &[("kb-0001", [3., 4.]), ("kb-0002", [1., 0.])])?;

    let mut previous = None;
    for _ in 0..2 {
        let query = "alpine qzxwplnr";
        let (output, requests) = run(
            root,
            &mut command(root, &["search", "--json", query]),
            &[(200, vectors(&[(0, [1., 0.])]))],
        )?;
        ok(&output);
        assert_eq!(requests, [json!([query])]);
        let value: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(value["rerank"], "agreement");
        assert_eq!(
            value["hits"],
            json!([
                { "id": "kb-0001", "path":"notes/kb-0001-fixture.md","title":"Alpine","snippet":"Body.", "match_kind":"partial","bm25_rank":1, "cosine":0.6,"cosine_rank":2,"fused_rank":1 },
                { "id": "kb-0002", "path":"notes/kb-0002-fixture.md","title":"Birch","snippet":"Second summary","match_kind":"semantic_only","bm25_rank":null,"cosine":1.0,"cosine_rank":1,"fused_rank":2 }
            ])
        );
        if let Some(before) = previous {
            assert_eq!(output.stdout, before);
        }
        previous = Some(output.stdout);
    }
    Ok(())
}

#[test]
fn search_text_keeps_original_banners_snippets_and_expansion_bytes() -> anyhow::Result<()> {
    let tmp = sandbox()?;
    let root = tmp.path();
    fs::create_dir(root.join("refs"))?;
    fs::write(
        root.join("refs/source.md"),
        "# Vintagemonad\n## Vintagemonad runs\n",
    )?;
    fs::write(
        root.join("notes/kb-0001-fixture.md"),
        "---\nid: kb-0001\ntitle: Alpine\nsupersedes: kb-0002\n---\nBody.\n",
    )?;
    fs::write(
        root.join("notes/kb-0002-fixture.md"),
        "---\nid: kb-0002\ntitle: Birch\nstatus: superseded\n---\nOld.\n",
    )?;
    fs::write(root.join("notes/kb-0003-fixture.md"), "---\nid: kb-0003\n")?;
    ok(&command(root, &["reindex"]).output()?);
    let alpine = "kb-0001  notes/kb-0001-fixture.md\n    Alpine\n    Body.\n\n";
    let partial = format!("(partial match — no note contains all query terms)\n{alpine}");
    for (args, expected) in [
        (vec!["--expand", "kb-0001"], format!("{alpine}(graph neighbours of top hits — depth 1, opt-in --expand:)\n    kb-0002  notes/kb-0002-fixture.md (superseded)\n        via supersedes: kb-0001 — Birch\n")),
        (vec!["KB-0002"], "kb-0002  notes/kb-0002-fixture.md ⚠SUPERSEDED\n    Birch\n    Old.\n\n".into()),
        (vec!["kb-0003"], "kb-0003  notes/kb-0003-fixture.md ⚠MALFORMED\n    unterminated frontmatter — `knowbox show` prints it raw\n\n".into()),
        (vec!["alpine"], alpine.into()),
        (vec!["alpi", "quokka"], partial.clone()),
        (vec!["lpi"], partial),
        (vec!["alpinez"], format!("(typo-corrected: 'alpinez'→'alpine')\n{alpine}")),
        (vec!["vintagemonad"], "refs/ mentions (not FTS-indexed — find the section-map note via `knowbox search` on the doc title):\n    refs/source.md: Vintagemonad / Vintagemonad runs\n".into()),
        (vec!["qzxqw"], "no matches. try: a synonym | broader term | `knowbox list`\n".into()),
    ] {
        let output = command(root, &["search"]).args(&args).output()?;
        ok(&output);
        assert_eq!(output.stderr, b"", "{args:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{args:?}");
    }
    Ok(())
}

#[test]
fn stale_no_write_search_reports_refresh_before_semantic_rescue() -> anyhow::Result<()> {
    let tmp = sandbox()?;
    let root = tmp.path();
    note(root, "kb-0001", "Alpine", "First summary", "active")?;
    ok(&command(root, &["reindex"]).output()?);
    seed_store(root, &[("kb-0001", [1., 0.])])?;
    let db = fs::read(root.join(".kb.db"))?;
    fs::write(root.join("index.md"), "stale fixture\n")?;
    let (output, requests) = run(
        root,
        &mut command(root, &["search", "--no-write", "--json", "qzxwplnr"]),
        &[],
    )?;
    assert_eq!(requests, Vec::<Value>::new());
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?,
        json!({
            "status":"stale_index","guidance":"search index is stale. Run 'knowbox reindex' to refresh."
        })
    );
    assert_eq!(output.stderr, b"");
    let output = command(root, &["search", "--no-write", "qzxwplnr"]).output()?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        output.stderr,
        b"search index is stale. Run 'knowbox reindex' to refresh.\n"
    );
    let direct = command(root, &["search", "--no-write", "--json", "kb-0001"]).output()?;
    ok(&direct);
    let direct: Value = serde_json::from_slice(&direct.stdout)?;
    assert_eq!(direct["index_status"], "unchecked");
    assert_eq!(direct["hits"][0]["path"], "notes/kb-0001-fixture.md");
    assert_eq!(fs::read(root.join(".kb.db"))?, db);
    assert_eq!(fs::read(root.join("index.md"))?, b"stale fixture\n");
    for path in [
        ".kb-usage",
        ".search-misses.log",
        ".kb.db-journal",
        ".kb.db-shm",
        ".kb.db-wal",
    ] {
        assert!(!root.join(path).try_exists()?, "{path}");
    }
    Ok(())
}
