use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use tempfile::TempDir;

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_knowbox"));
    for key in [
        "KNOWBOX_ROOT",
        "KNOWBOX_EMBED_URL",
        "KNOWBOX_EMBED_MODEL",
        "KNOWBOX_API_KEY",
    ] {
        command.env_remove(key);
    }
    command
        .env("KNOWBOX_HOST", "explorer-test")
        .env("KNOWBOX_NO_HEARTBEAT", "1");
    command
}

fn fixture() -> Result<(TempDir, PathBuf)> {
    let directory = tempfile::tempdir()?;
    let kb = directory.path().join("kb");
    let init = command().args(["init", "--no-git"]).arg(&kb).output()?;
    ensure!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let note = command()
        .env("KNOWBOX_ROOT", &kb)
        .args([
            "new", "--type", "fact", "--domain", "fixture", "--summary", "Test the Explorer",
            "--title", "Explorer note", "--body",
            "querycanary\n\n<script>alert(1)</script>\n\n[bad](javascript:alert%281%29) ![image](https://example.org/a.png) [[kb-0001]]",
            "--no-edit",
        ])
        .output()?;
    ensure!(
        note.status.success(),
        "new failed: {}",
        String::from_utf8_lossy(&note.stderr)
    );
    Ok((directory, kb))
}

struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        if let Err(error) = self.0.kill()
            && error.kind() != std::io::ErrorKind::InvalidInput
        {
            eprintln!("stopping fixture Explorer: {error}");
        }
        if let Err(error) = self.0.wait() {
            eprintln!("reaping fixture Explorer: {error}");
        }
    }
}

struct Explorer {
    _running: Running,
    address: SocketAddrV4,
    token: String,
}

impl Explorer {
    fn start(root: &Path) -> Result<Self> {
        Self::start_with(root, &["--no-write", "serve", "--read-only"])
    }

    fn start_with(root: &Path, args: &[&str]) -> Result<Self> {
        let mut running = Running(
            command()
                .env("KNOWBOX_ROOT", root)
                .args(args)
                .args(["--port", "0"])
                .stdout(Stdio::piped())
                .spawn()?,
        );
        let mut url = String::new();
        BufReader::new(
            running
                .0
                .stdout
                .take()
                .context("missing Explorer launch output")?,
        )
        .read_line(&mut url)?;
        let (address, token) = url
            .trim_end()
            .strip_prefix("http://")
            .and_then(|url| url.split_once("/#token="))
            .context("invalid Explorer launch URL")?;
        let address: SocketAddrV4 = address.parse()?;
        assert_eq!(address.ip(), &Ipv4Addr::LOCALHOST);
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        Ok(Self {
            _running: running,
            address,
            token: token.to_owned(),
        })
    }

    fn request(&self, method: &str, path: &str, headers: &[(&str, &str)]) -> Result<Http> {
        self.request_body(method, path, headers, "")
    }

    fn request_body(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<Http> {
        let mut conn = TcpStream::connect(self.address)?;
        conn.set_read_timeout(Some(Duration::from_secs(5)))?;
        conn.set_write_timeout(Some(Duration::from_secs(5)))?;
        write!(conn, "{method} {path} HTTP/1.1\r\nConnection: close\r\n")?;
        for (key, value) in headers {
            write!(conn, "{key}: {value}\r\n")?;
        }
        if !body.is_empty() {
            write!(conn, "Content-Length: {}\r\n", body.len())?;
        }
        write!(conn, "\r\n{body}")?;
        let mut raw = String::new();
        conn.read_to_string(&mut raw)?;
        let (headers, body) = raw
            .split_once("\r\n\r\n")
            .context("invalid HTTP response")?;
        let status = headers
            .split_ascii_whitespace()
            .nth(1)
            .context("missing HTTP status")?
            .parse()?;
        Ok(Http {
            status,
            headers: headers.to_owned(),
            body: body.to_owned(),
        })
    }

    fn get(&self, path: &str) -> Result<Http> {
        self.request(
            "GET",
            path,
            &[
                ("Host", &self.address.to_string()),
                ("Authorization", &format!("Bearer {}", self.token)),
            ],
        )
    }

    fn post(&self, path: &str, value: Value) -> Result<Http> {
        self.request_body(
            "POST",
            path,
            &[
                ("Host", &self.address.to_string()),
                ("Origin", &format!("http://{}", self.address)),
                ("Authorization", &format!("Bearer {}", self.token)),
                ("Content-Type", "application/json"),
            ],
            &value.to_string(),
        )
    }
}

struct Http {
    status: u16,
    headers: String,
    body: String,
}

impl Http {
    fn json(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.body)?)
    }
}

#[test]
fn writable_explorer_checks_origin_token_and_read_only_mode() -> Result<()> {
    let (_dir, kb) = fixture()?;
    let read = Explorer::start_with(&kb, &["--no-write", "serve"])?;
    assert_eq!(read.get("/api/capabilities")?.json()?["writable"], false);
    assert_eq!(read.post("/api/write", serde_json::json!({}))?.status, 405);
    assert_eq!(read.get("/api/editor?target=kb-0001")?.status, 403);

    let write = Explorer::start_with(&kb, &["serve"])?;
    assert_eq!(write.get("/api/capabilities")?.json()?["writable"], true);
    let host = write.address.to_string();
    let origin = format!("http://{host}");
    let token = format!("Bearer {}", write.token);
    let inputs = [
        (
            403,
            vec![("Host", host.as_str()), ("Authorization", token.as_str())],
        ),
        (
            403,
            vec![
                ("Host", host.as_str()),
                ("Origin", origin.as_str()),
                ("Origin", origin.as_str()),
                ("Authorization", token.as_str()),
            ],
        ),
        (
            401,
            vec![("Host", host.as_str()), ("Origin", origin.as_str())],
        ),
        (
            415,
            vec![
                ("Host", host.as_str()),
                ("Origin", origin.as_str()),
                ("Authorization", token.as_str()),
                ("Content-Type", "text/plain"),
            ],
        ),
    ];
    for (status, headers) in inputs {
        assert_eq!(
            write
                .request_body("POST", "/api/write", &headers, "{}")?
                .status,
            status
        );
    }
    assert!(!kb.join(".kb-pending-write.json").exists());
    Ok(())
}

#[test]
fn editor_compares_raw_bytes_validates_drafts_and_returns_both_versions() -> Result<()> {
    let (_dir, kb) = fixture()?;
    let path = kb.join("notes/kb-0001-explorer-note.md");
    let original = fs::read_to_string(&path)?.replace('\n', "\r\n");
    fs::write(&path, &original)?;
    let explorer = Explorer::start_with(&kb, &["--client", "browser-test", "serve"])?;
    let source = explorer.get("/api/editor?target=kb-0001")?.json()?;
    assert_eq!(source["markdown"], original);
    let edited = original.replace("querycanary", "updated note");
    let bad = edited.replace("status: active", "status: invalid");
    let draft = explorer
        .post(
            "/api/draft",
            serde_json::json!({ "path":source["path"], "markdown":bad }),
        )?
        .json()?;
    assert_eq!(draft["lint"]["errors"], 1);
    assert!(
        draft["body_html"]
            .as_str()
            .context("preview HTML")?
            .contains("&lt;script&gt;")
    );
    let edit = |key: &str, body: &str| {
        serde_json::json!({
            "request_id":key,
            "operation":{
                "action":"edit", "selector":source["path"], "base_sha1":source["base_sha1"],
                "input":{"format":"source","markdown":body}
            }
        })
    };
    let unchanged = explorer.post("/api/write", edit("noop-once", &original))?;
    assert_eq!(unchanged.status, 200);
    assert_eq!(unchanged.json()?["changed"], false);
    let rejected = explorer.post("/api/write", edit("invalid", &bad))?;
    assert_eq!(rejected.status, 422);
    assert_eq!(rejected.json()?["code"], "lint");
    assert_eq!(fs::read_to_string(&path)?, original);
    let saved = explorer.post("/api/write", edit("edit-once", &edited))?;
    assert_eq!(saved.status, 200, "{}", saved.body);
    assert_eq!(saved.json()?["git"]["state"], "no_repository");
    assert_eq!(fs::read_to_string(&path)?, edited);
    assert!(
        fs::read_to_string(kb.join("log.md"))?.contains(" edit | kb-0001 | client=browser-test | ")
    );
    let replay = explorer
        .post("/api/write", edit("edit-once", &edited))?
        .json()?;
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["note"]["path"], source["path"]);
    let conflict = explorer.post("/api/write", edit("stale", &original))?;
    assert_eq!(conflict.status, 409);
    assert_eq!(conflict.json()?["current_markdown"], edited);
    assert_eq!(
        explorer
            .post("/api/write", edit("noop-once", &original))?
            .json()?["replayed"],
        true
    );
    assert_eq!(
        explorer
            .post("/api/write", edit("noop-once", &edited))?
            .json()?["code"],
        "request_key_reused"
    );
    Ok(())
}

fn new_draft(title: &str, body: &str) -> String {
    format!(
        "---\nid: pending\ntitle: {title}\ntype: fact\ndomain: fixture\nstatus: active\nconfidence: high\ncreated: pending\nupdated: pending\nsummary: A complete note\nrelated: []\nsupersedes: []\n---\n{body}\n"
    )
}

#[test]
fn review_ingest_links_both_notes_and_reuses_the_allocated_id_after_restart() -> Result<()> {
    let (_dir, kb) = fixture()?;
    let path = kb.join("raw/proposals.md");
    let raw = "# Proposals\r\n\r\n## A new note\r\nProposal text.\r\n";
    fs::write(&path, raw)?;
    let explorer = Explorer::start_with(&kb, &["serve"])?;
    let backlog = explorer.get("/api/raw-backlog")?.json()?;
    let proposal = &backlog["entries"][0];
    let request = serde_json::json!({
        "request_id":"ingest-once",
        "operation":{
            "action":"create", "markdown":new_draft("A new note","Proposal text."),
            "related":["kb-0001"], "inbox":{"file":proposal["file"], "number":proposal["number"], "revision":proposal["revision"]}
        }
    });
    let saved = explorer.post("/api/write", request.clone())?;
    assert_eq!(saved.status, 200, "{}", saved.body);
    assert_eq!(saved.json()?["note"]["id"], "kb-0002");
    assert_eq!(explorer.get("/api/raw-backlog")?.json()?["unreviewed"], 0);
    assert!(
        fs::read_to_string(kb.join("notes/kb-0001-explorer-note.md"))?
            .contains("related: [kb-0002]")
    );
    assert!(
        fs::read_to_string(kb.join("notes/kb-0002-a-new-note.md"))?.contains("related: [kb-0001]")
    );
    let marked = fs::read_to_string(&path)?;
    assert_eq!(marked.replace("\r\n", "").matches('\n').count(), 0);
    assert_eq!(marked.matches("> state: ingested kb-0002").count(), 1);
    let journal = fs::read(kb.join("log.md"))?;
    drop(explorer);
    let restarted = Explorer::start_with(&kb, &["serve"])?;
    let replay = restarted.post("/api/write", request)?.json()?;
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["note"]["id"], "kb-0002");
    assert_eq!(journal, fs::read(kb.join("log.md"))?);
    assert_eq!(
        restarted.get("/api/list")?.json()?["notes"]
            .as_array()
            .context("notes")?
            .len(),
        2
    );
    Ok(())
}

#[test]
fn split_and_supersede_check_the_original_hash_and_save_both_parts() -> Result<()> {
    let (_dir, kb) = fixture()?;
    let explorer = Explorer::start_with(&kb, &["serve"])?;
    let source = explorer.get("/api/editor?target=kb-0001")?.json()?;
    let split = explorer.post("/api/write", serde_json::json!({
        "request_id":"split",
        "operation":{"action":"create", "markdown":new_draft("Split note","Moved detail."),
            "followup":{"kind":"split","path":source["path"],"base_sha1":source["base_sha1"],"remaining":"Only the original claim.\n"}}
    }))?;
    assert_eq!(split.status, 200, "{}", split.body);
    let path = kb.join(source["path"].as_str().context("source path")?);
    let block_list =
        fs::read_to_string(&path)?.replace("related: [kb-0002]", "related:\n  - kb-0002");
    fs::write(path, block_list)?;
    let original = explorer.get("/api/editor?target=kb-0001")?.json()?;
    assert_eq!(original["body"], "Only the original claim.\n");
    assert_eq!(
        original["frontmatter"]["related"],
        serde_json::json!(["kb-0002"])
    );
    let newer = explorer.get("/api/editor?target=kb-0002")?.json()?;
    assert_eq!(
        newer["frontmatter"]["related"],
        serde_json::json!(["kb-0001"])
    );
    let successor = |base: &Value| {
        serde_json::json!({
            "request_id":"successor",
            "operation":{"action":"create", "markdown":new_draft("New claim","The revised claim."),
                "followup":{"kind":"supersede","path":source["path"],"base_sha1":base,"reason":"New evidence corrected the claim"}}
        })
    };
    assert_eq!(
        explorer
            .post("/api/write", successor(&source["base_sha1"]))?
            .status,
        409
    );
    assert_eq!(
        explorer.get("/api/list")?.json()?["notes"]
            .as_array()
            .context("notes")?
            .len(),
        2
    );
    let saved = explorer.post("/api/write", successor(&original["base_sha1"]))?;
    assert_eq!(saved.status, 200, "{}", saved.body);
    assert_eq!(
        explorer.get("/api/editor?target=kb-0001")?.json()?["frontmatter"]["status"],
        "superseded"
    );
    assert_eq!(
        explorer.get("/api/editor?target=kb-0001")?.json()?["frontmatter"]["related"],
        serde_json::json!(["kb-0002", "kb-0003"])
    );
    assert_eq!(
        explorer.get("/api/editor?target=kb-0003")?.json()?["frontmatter"]["supersedes"],
        serde_json::json!(["kb-0001"])
    );
    Ok(())
}

#[test]
fn browser_verdict_refuses_changed_claims_and_resolves_the_reviewed_pair_once() -> Result<()> {
    let (_dir, kb) = fixture()?;
    let explorer = Explorer::start_with(&kb, &["serve"])?;
    let created = explorer.post("/api/write", serde_json::json!({
        "request_id":"second", "operation":{"action":"create", "markdown":new_draft("Second note","The conflicting claim.")}
    }))?;
    assert_eq!(created.status, 200);
    let a = explorer.get("/api/editor?target=kb-0001")?.json()?;
    let old = explorer.get("/api/editor?target=kb-0002")?.json()?;
    let b_path = kb.join(old["path"].as_str().context("note path")?);
    let text = fs::read_to_string(&b_path)?.replace("The conflicting claim.", "A corrected claim.");
    fs::write(&b_path, &text)?;
    let decide = |b: &Value| {
        serde_json::json!({
            "request_id":"review-pair",
            "operation":{"action":"verdict", "a":a["path"], "b":b["path"], "bases":{"a":a["base_sha1"],"b":b["base_sha1"]},
                "verdict":"contradiction","winner":"kb-0001","reason":"Evidence for the first note"}
        })
    };
    let stale = explorer.post("/api/write", decide(&old))?;
    assert_eq!(stale.status, 409);
    assert_eq!(stale.json()?["current_markdown"], text);
    assert_eq!(fs::read_to_string(&b_path)?, text);
    assert!(
        fs::read_to_string(kb.join("log.md"))?
            .lines()
            .all(|line| !line.contains(" verdict | "))
    );
    let current = explorer.get("/api/editor?target=kb-0002")?.json()?;
    let saved = explorer.post("/api/write", decide(&current))?;
    assert_eq!(saved.status, 200, "{}", saved.body);
    assert_eq!(
        explorer.get("/api/editor?target=kb-0002")?.json()?["frontmatter"]["status"],
        "superseded"
    );
    assert_eq!(
        explorer.get("/api/editor?target=kb-0001")?.json()?["frontmatter"]["supersedes"],
        serde_json::json!(["kb-0002"])
    );
    assert_eq!(
        explorer.post("/api/write", decide(&current))?.json()?["replayed"],
        true
    );
    Ok(())
}

#[test]
fn explorer_checks_host_origin_and_a_per_start_token_before_data() -> Result<()> {
    let (_directory, kb) = fixture()?;
    let explorer = Explorer::start(&kb)?;
    let second = Explorer::start(&kb)?;
    assert_ne!(explorer.token, second.token);
    let host = explorer.address.to_string();
    let token = format!("Bearer {}", explorer.token);
    let origin = format!("http://{host}");
    let list = "/api/list";
    assert_eq!(
        explorer.request("GET", list, &[("Host", &host)])?.status,
        401
    );
    assert_eq!(
        explorer
            .request(
                "GET",
                list,
                &[("Host", "example.org"), ("Authorization", &token)]
            )?
            .status,
        403,
    );
    assert_eq!(
        explorer
            .request(
                "GET",
                list,
                &[("Host", &host), ("Host", &host), ("Authorization", &token)]
            )?
            .status,
        403,
    );
    for wrong in ["null", "http://example.org"] {
        assert_eq!(
            explorer
                .request(
                    "GET",
                    list,
                    &[
                        ("Host", &host),
                        ("Origin", wrong),
                        ("Authorization", &token)
                    ]
                )?
                .status,
            403,
        );
    }
    assert_eq!(
        explorer
            .request(
                "GET",
                list,
                &[
                    ("Host", &host),
                    ("Origin", &origin),
                    ("Authorization", &token)
                ]
            )?
            .status,
        200,
    );
    assert_eq!(
        explorer
            .request("POST", list, &[("Host", &host), ("Authorization", &token)])?
            .status,
        405,
    );
    assert_eq!(
        second
            .request(
                "GET",
                list,
                &[
                    ("Host", &second.address.to_string()),
                    ("Authorization", &token)
                ]
            )?
            .status,
        401,
    );
    let shell = explorer.request("GET", "/", &[("Host", &host)])?;
    assert_eq!(shell.status, 404);
    assert!(shell.body.contains("knowbox-server web bundle"));
    assert!(!shell.body.contains(&explorer.token));
    assert!(shell.headers.contains("Content-Security-Policy:"));
    assert!(shell.headers.contains("Cache-Control: no-store"));
    assert_eq!(explorer.get("/api/capabilities")?.json()?["api_version"], 1);
    assert_eq!(explorer.get("/api/ref?path=%ZZ")?.status, 400);
    assert_eq!(explorer.get("/api/search?q=one&q=two")?.status, 400);
    Ok(())
}

#[test]
fn local_launch_reads_only_assets_from_the_explicit_web_bundle() -> Result<()> {
    let (dir, kb) = fixture()?;
    let web = dir.path().join("web");
    fs::create_dir(&web)?;
    fs::write(
        web.join("index.html"),
        "<title>External test bundle</title>",
    )?;
    fs::write(web.join("app.js"), "console.log('bundle loaded');")?;
    fs::write(dir.path().join("private.js"), "outside bundle")?;
    let explorer = Explorer::start_with(
        &kb,
        &[
            "serve",
            "--assets",
            web.to_str().context("UTF-8 test bundle path")?,
        ],
    )?;
    let host = explorer.address.to_string();
    let get = |path: &str| explorer.request("GET", path, &[("Host", &host)]);
    assert_eq!(get("/")?.body, "<title>External test bundle</title>");
    assert_eq!(get("/editor")?.body, "<title>External test bundle</title>");
    assert_eq!(get("/assets/app.js")?.body, "console.log('bundle loaded');");
    assert_eq!(get("/assets/../private.js")?.status, 404);
    assert_eq!(get("/assets/missing.js")?.status, 404);
    Ok(())
}

#[test]
fn explorer_reads_typed_notes_and_detects_changes_without_writing() -> Result<()> {
    let (_directory, kb) = fixture()?;
    fs::write(
        kb.join("refs/field % Ω.md"),
        "# Reference\n\n## First\nText\n### Nested\nDetail\n## Second\nMore\n",
    )?;
    let rebuild = command().env("KNOWBOX_ROOT", &kb).arg("reindex").output()?;
    assert!(rebuild.status.success());
    let db = fs::read(kb.join(".kb.db"))?;
    let index = fs::read(kb.join("index.md"))?;
    let log = fs::read(kb.join("log.md"))?;
    let lock_path = kb.join(".kb.lock");
    let lock = fs::File::options().write(true).open(&lock_path)?;
    lock.lock()?;

    let explorer = Explorer::start(&kb)?;
    let query = "/api/search?q=querycanary&local=1";
    let result = explorer.get(query)?.json()?;
    assert_eq!(result["hits"][0]["id"], "kb-0001");
    assert_eq!(result["hits"][0]["path"], "notes/kb-0001-explorer-note.md");
    assert_eq!(
        explorer.get("/api/search?q=missingwordzz")?.json()?["layer"],
        "miss"
    );
    let detail = explorer
        .get("/api/note?path=notes%2Fkb-0001-explorer-note.md")?
        .json()?;
    assert_eq!(detail["id"], "kb-0001");
    let html = detail["body_html"]
        .as_str()
        .context("missing Markdown preview")?;
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("href=\"/n/kb-0001\""));
    assert!(!html.contains("<script") && !html.contains("<img") && !html.contains("javascript:"));
    let reference = explorer
        .get("/api/ref?path=refs%2Ffield+%25+%CE%A9.md&line=3")?
        .json()?;
    assert_eq!(reference["start_line"], 3);
    assert_eq!(reference["end_line"], 6);
    assert_eq!(
        reference["markdown"],
        "## First\nText\n### Nested\nDetail\n"
    );
    assert_eq!(
        explorer.get("/api/ref?path=refs%2F..%2Flog.md")?.status,
        404
    );
    assert_eq!(
        explorer.get("/api/note?path=notes%2F..%2Flog.md")?.status,
        404
    );

    let revision = "/api/revision?view=search";
    let before = explorer.get(revision)?.json()?;
    assert_eq!(explorer.get(revision)?.json()?, before);
    let path = kb.join("notes/kb-0001-explorer-note.md");
    let original = fs::read_to_string(&path)?;
    let mtime = path.metadata()?.modified()?;
    let changed = original.replace("querycanary", "othercanary");
    assert_eq!(original.len(), changed.len());
    fs::write(&path, changed)?;
    fs::File::options()
        .write(true)
        .open(&path)?
        .set_modified(mtime)?;
    assert_ne!(explorer.get(revision)?.json()?, before);
    assert_eq!(explorer.get(query)?.json()?["status"], "stale_index");
    let direct = explorer.get("/api/search?q=kb-0001")?.json()?;
    assert_eq!(direct["index_status"], "unchecked");
    assert_eq!(fs::read(kb.join(".kb.db"))?, db);
    assert_eq!(fs::read(kb.join("index.md"))?, index);
    assert_eq!(fs::read(kb.join("log.md"))?, log);
    assert!(!kb.join(".kb-usage").exists());
    assert!(!kb.join(".search-misses.log").exists());
    assert!(!kb.join(".kb-pending-write.json").exists());
    Ok(())
}

#[test]
fn review_revision_tracks_local_usage_and_misses() -> Result<()> {
    let (_directory, kb) = fixture()?;
    let explorer = Explorer::start(&kb)?;
    let revision = "/api/revision?view=review";
    let before = explorer.get(revision)?.json()?;
    let show = command()
        .env("KNOWBOX_ROOT", &kb)
        .args(["show", "kb-0001"])
        .output()?;
    ensure!(show.status.success(), "fixture show failed");
    assert!(kb.join(".kb-usage").is_dir());
    let with_usage = explorer.get(revision)?.json()?;
    assert_ne!(with_usage, before);
    fs::write(
        kb.join(".search-misses.log"),
        "2026-05-01T12:00:00+00:00\tcanary\n",
    )?;
    assert_ne!(explorer.get(revision)?.json()?, with_usage);
    Ok(())
}

#[test]
fn api_and_cli_share_context_map_and_daily_metrics_without_writing_on_reads() -> Result<()> {
    let (_dir, kb) = fixture()?;
    let cli = |args: &[&str]| -> Result<Value> {
        let output = command().env("KNOWBOX_ROOT", &kb).args(args).output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let sweep = command().env("KNOWBOX_ROOT", &kb).arg("sweep").output()?;
    ensure!(
        sweep.status.success(),
        "{}",
        String::from_utf8_lossy(&sweep.stderr)
    );
    let metric = kb.join(".kb-metrics.jsonl");
    let before = fs::read(&metric)?;
    let explorer = Explorer::start(&kb)?;
    assert_eq!(
        explorer
            .get("/api/context?project=%20&host=fixture")?
            .status,
        400
    );
    let first = explorer
        .get("/api/context?project=fixture&host=explorer-test&explain=1")?
        .json()?;
    assert_eq!(
        first,
        cli(&[
            "context",
            "--project",
            "fixture",
            "--host",
            "explorer-test",
            "--explain",
            "--json"
        ])?
    );
    assert_eq!(first["items"][0]["id"], "kb-0001");
    assert_eq!(
        first["characters"],
        first["text"].as_str().context("text")?.chars().count()
    );
    let map = explorer.get("/api/map")?.json()?;
    assert_eq!(map, cli(&["map", "--json"])?);
    assert_eq!(map["mode"], "graph");
    assert_eq!(map["nodes"][0]["id"], "kb-0001");
    assert_eq!(map["misses"]["state"], "offline");
    let measurements = explorer.get("/api/metrics")?.json()?;
    assert_eq!(measurements, cli(&["metrics", "--json"])?);
    assert_eq!(
        measurements["rows"]
            .as_array()
            .context("measurements")?
            .len(),
        1
    );
    assert_eq!(fs::read(&metric)?, before);
    assert!(!kb.join(".search-misses.log").exists());
    Ok(())
}
