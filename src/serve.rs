mod api;
mod markdown;
mod revision;
mod write_api;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Cursor, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use percent_encoding::percent_decode_str;
use subtle::ConstantTimeEq;
use tempfile::TempDir;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::config::KbRoot;
use crate::journal::Actor;

const POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; frame-src 'self'; base-uri 'none'; frame-ancestors 'none'; object-src 'none'; form-action 'none'";
const DIAGRAM_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'unsafe-inline'; img-src data:; frame-ancestors 'self'; sandbox allow-scripts; base-uri 'none'; object-src 'none'; form-action 'none'";
const MAX_TARGET: usize = 8192;
const API_VERSION: u8 = 1;

#[derive(Debug, clap::Args)]
pub(crate) struct Options {
    /// Listen on this loopback port. Use 0 to select a free port.
    #[arg(long, default_value_t = 7878)]
    port: u16,
    /// Serve the Explorer in read-only mode.
    #[arg(long = "read-only")]
    pub(crate) read_only: bool,
    /// Serve the separately installed Explorer web bundle from this directory.
    #[arg(long, value_name = "DIRECTORY")]
    assets: Option<PathBuf>,
    /// Open the launch URL in the default browser on this machine.
    #[arg(long, requires = "assets")]
    open: bool,
}

struct Access {
    host: String,
    origin: String,
    token: String,
    writer: Option<Actor>,
    assets: Option<PathBuf>,
}

impl Access {
    fn validate(&self, request: &Request) -> Result<(), Reply> {
        let valid_host = matches!(
            header(request, "Host"),
            Ok(Some(host))
                if host == self.host || (self.host == "127.0.0.1" && host == "127.0.0.1:80")
        );
        if !valid_host {
            return Err(Reply::error(403, "invalid Host"));
        }
        match header(request, "Origin") {
            Ok(None) => {}
            Ok(Some(origin)) if origin == self.origin => {}
            _ => return Err(Reply::error(403, "invalid Origin")),
        }
        Ok(())
    }

    fn authorized(&self, request: &Request) -> bool {
        let Ok(Some(value)) = header(request, "Authorization") else {
            return false;
        };
        let Some(token) = value.strip_prefix("Bearer ") else {
            return false;
        };
        bool::from(self.token.as_bytes().ct_eq(token.as_bytes()))
    }
}

fn header<'a>(request: &'a Request, name: &'static str) -> Result<Option<&'a str>, ()> {
    let mut values = request
        .headers()
        .iter()
        .filter(|item| item.field.equiv(name));
    let value = values.next().map(|item| item.value.as_str());
    if values.next().is_some() {
        Err(())
    } else {
        Ok(value)
    }
}

pub(crate) fn run(
    root: &KbRoot,
    options: &Options,
    client: Option<&str>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let assets = options
        .assets
        .as_deref()
        .map(|path| asset_root(path, root))
        .transpose()?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, options.port))
        .context("binding the Knowbox Explorer to IPv4 loopback")?;
    let host = authority(listener.local_addr()?);
    let mut random = [0; 32];
    getrandom::getrandom(&mut random)
        .map_err(|error| anyhow::anyhow!("generating the Explorer token: {error}"))?;
    let mut token = String::with_capacity(64);
    for byte in random {
        write!(token, "{byte:02x}")?;
    }
    let access = Access {
        origin: format!("http://{host}"),
        host,
        token,
        writer: (!options.read_only)
            .then(|| Actor::resolve(client, "explorer"))
            .transpose()?,
        assets,
    };
    let server = Server::from_listener(listener, None)
        .map_err(|error| anyhow::anyhow!("starting the Explorer: {error}"))?;
    let url = format!("{}/#token={}", access.origin, access.token);
    writeln!(out, "{url}")?;
    out.flush()?;
    let mut launch_dir = None;
    let mut browser = None;
    if options.open {
        match launch_browser(root, &url) {
            Ok((directory, child)) => {
                launch_dir = Some(directory);
                browser = Some(child);
            }
            Err(error) => writeln!(err, "knowbox serve: could not open the browser: {error:#}")?,
        }
    }
    loop {
        if let Some(child) = &mut browser
            && let Some(status) = child.try_wait().context("checking the browser launcher")?
        {
            browser = None;
            if !status.success() {
                writeln!(err, "knowbox serve: browser launcher exited with {status}")?;
            }
        }
        let request = if browser.is_some() {
            server.recv_timeout(Duration::from_millis(250))
        } else {
            server.recv().map(Some)
        }
        .context("receiving an Explorer request")?;
        let Some(mut request) = request else { continue };
        let reply = handle(&access, root, &mut request);
        if reply.status == 200
            && request.url().starts_with("/api/")
            && let Some(directory) = launch_dir.take()
            && let Err(error) = directory.close()
        {
            writeln!(
                err,
                "knowbox serve: removing the browser launch document: {error}"
            )?;
        }
        if let Err(error) = request.respond(reply.response()?)
            && !matches!(
                error.kind(),
                io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
            )
        {
            writeln!(err, "knowbox serve: responding to the browser: {error}")?;
        }
    }
}

fn authority(address: SocketAddr) -> String {
    if address.port() == 80 {
        address.ip().to_string()
    } else {
        address.to_string()
    }
}

fn launch_browser(root: &KbRoot, url: &str) -> Result<(TempDir, Child)> {
    let base =
        fs::canonicalize(std::env::temp_dir()).context("resolving the temporary directory")?;
    ensure!(
        !base.starts_with(root.path()),
        "the browser launch document needs a temporary directory outside the knowledge base"
    );
    let mut builder = tempfile::Builder::new();
    builder.prefix("knowbox-explorer-");
    let mut options = fs::File::options();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        builder.permissions(fs::Permissions::from_mode(0o700));
        options.mode(0o600);
    }
    let directory = builder.tempdir_in(base)?;
    let path = directory.path().join("open.html");
    writeln!(
        options.open(&path)?,
        "<!doctype html><meta name=\"referrer\" content=\"no-referrer\"><meta http-equiv=\"refresh\" content=\"0;url={url}\"><title>Opening Knowbox</title>"
    )?;

    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");
    let child = command
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok((directory, child))
}

fn handle(access: &Access, root: &KbRoot, request: &mut Request) -> Reply {
    if let Err(reply) = access.validate(request) {
        return reply;
    }
    if request.url().len() > MAX_TARGET {
        return Reply::error(414, "request target is too long");
    }
    if request.method() == &Method::Post {
        return handle_post(access, root, request);
    }
    if !matches!(request.method(), Method::Get | Method::Head) {
        return Reply::method("GET, HEAD");
    }
    let (path, query) = request.url().split_once('?').unwrap_or((request.url(), ""));
    if path.starts_with("/api/") {
        if !access.authorized(request) {
            return Reply::error(401, "open the launch URL printed by knowbox serve");
        }
        let query = match Query::parse(query) {
            Ok(query) => query,
            Err(error) => return Reply::error(400, &error.to_string()),
        };
        if path == "/api/capabilities" {
            return Reply::json(
                200,
                &serde_json::json!({ "api_version": API_VERSION, "writable": access.writer.is_some(), "insights": true }),
            )
            .unwrap_or_else(|error| Reply::error(500, &error.to_string()));
        }
        if path == "/api/editor" {
            return if access.writer.is_some() {
                write_api::source(root, &query)
            } else {
                Reply::error(403, "the Explorer is read-only")
            };
        }
        return api::read(root, path, &query)
            .unwrap_or_else(|error| Reply::error(500, &format!("{error:#}")));
    }
    if let Some(directory) = &access.assets {
        return match read_asset(directory, path) {
            Ok(Some(reply)) => reply,
            Ok(None) => Reply::error(404, "page not found"),
            Err(error) => Reply::error(500, &format!("{error:#}")),
        };
    }
    if path == "/" {
        Reply::error(
            404,
            "this process serves the Knowbox API; pass --assets with the knowbox-server web bundle to open the Explorer",
        )
    } else {
        Reply::error(404, "page not found")
    }
}

fn asset_root(path: &Path, kb: &KbRoot) -> Result<PathBuf> {
    let label = path.display();
    let root = path
        .canonicalize()
        .with_context(|| format!("opening the Explorer bundle at {label}"))?;
    ensure!(
        !root.starts_with(kb.path()),
        "the Explorer bundle must be outside the knowledge base"
    );
    ensure!(
        crate::store::directory(&root)? && crate::store::regular_file(&root.join("index.html"))?,
        "expected an Explorer bundle with index.html at {label}"
    );
    Ok(root)
}

fn read_asset(root: &Path, url: &str) -> Result<Option<Reply>> {
    let (name, mime) = if let Some(name) = url.strip_prefix("/assets/") {
        if name.starts_with('.')
            || name.contains("..")
            || !name
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_' | b'.'))
        {
            return Ok(None);
        }
        let mime = match name.rsplit_once('.').map(|(_, ext)| ext) {
            Some("js") => "text/javascript",
            Some("css") => "text/css",
            Some("html") => "text/html",
            Some("LICENSE" | "txt") => "text/plain",
            _ => return Ok(None),
        };
        (name, mime)
    } else if url.starts_with('/')
        && url
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_' | b'/'))
    {
        ("index.html", "text/html")
    } else {
        return Ok(None);
    };
    let path = root.join(name);
    if !crate::store::regular_file(&path)? {
        return Ok(None);
    }
    let bytes =
        crate::store::read_regular(&path).with_context(|| format!("reading asset {name}"))?;
    let mut reply = Reply::asset(mime, bytes);
    if name == "diagram.html" {
        reply.policy = DIAGRAM_POLICY;
    } else if matches!(name, "diagram-frame.js" | "mermaid.min.js") {
        // The sandboxed frame has an opaque origin.
        reply.resource_policy = "cross-origin";
    }
    Ok(Some(reply))
}

fn handle_post(access: &Access, root: &KbRoot, request: &mut Request) -> Reply {
    let Some(actor) = &access.writer else {
        return Reply::method("GET, HEAD");
    };
    let route = match request.url() {
        "/api/write" => write_api::Route::Write,
        "/api/draft" => write_api::Route::Draft,
        _ => return Reply::method("GET, HEAD"),
    };
    if header(request, "Origin") != Ok(Some(access.origin.as_str())) {
        return Reply::error(403, "mutations require the Explorer Origin");
    }
    if !access.authorized(request) {
        return Reply::error(401, "open the launch URL printed by knowbox serve");
    }
    write_api::post(root, actor, route, request)
}

struct Reply {
    status: u16,
    mime: &'static str,
    body: Vec<u8>,
    allow: Option<&'static str>,
    policy: &'static str,
    resource_policy: &'static str,
}

impl Reply {
    fn asset(mime: &'static str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            mime,
            body,
            allow: None,
            policy: POLICY,
            resource_policy: "same-origin",
        }
    }

    fn json(status: u16, value: &impl serde::Serialize) -> Result<Self> {
        Ok(Self {
            status,
            mime: "application/json",
            body: serde_json::to_vec(value)?,
            allow: None,
            policy: POLICY,
            resource_policy: "same-origin",
        })
    }

    fn error(status: u16, message: &str) -> Self {
        let body = serde_json::json!({ "error": message })
            .to_string()
            .into_bytes();
        Self {
            status,
            mime: "application/json",
            body,
            allow: None,
            policy: POLICY,
            resource_policy: "same-origin",
        }
    }

    fn method(allow: &'static str) -> Self {
        Self {
            allow: Some(allow),
            ..Self::error(405, "method is unavailable for this route")
        }
    }

    fn response(self) -> Result<Response<Cursor<Vec<u8>>>> {
        let mut response = Response::from_data(self.body).with_status_code(StatusCode(self.status));
        for (name, value) in [
            ("Content-Type", format!("{}; charset=utf-8", self.mime)),
            ("Cache-Control", "no-store".into()),
            ("Content-Security-Policy", self.policy.into()),
            ("Referrer-Policy", "no-referrer".into()),
            ("X-Content-Type-Options", "nosniff".into()),
            ("Cross-Origin-Resource-Policy", self.resource_policy.into()),
        ] {
            response.add_header(
                Header::from_bytes(name, value)
                    .map_err(|()| anyhow::anyhow!("invalid Explorer response header {name}"))?,
            );
        }
        if let Some(allow) = self.allow {
            response.add_header(
                Header::from_bytes("Allow", allow)
                    .map_err(|()| anyhow::anyhow!("invalid Allow header"))?,
            );
        }
        Ok(response)
    }
}

struct Query(BTreeMap<String, String>);

impl Query {
    fn parse(query: &str) -> Result<Self> {
        let mut values = BTreeMap::new();
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').context("invalid query parameter")?;
            let key = decode(key)?;
            let value = decode(value)?;
            ensure!(
                values.insert(key, value).is_none(),
                "duplicate query parameter"
            );
        }
        Ok(Self(values))
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    fn flag(&self, key: &str, default: bool) -> Result<bool> {
        match self.get(key) {
            Some("0") => Ok(false),
            Some("1") => Ok(true),
            None => Ok(default),
            Some(_) => bail!("{key} must be 0 or 1"),
        }
    }
}

fn decode(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    for (index, _) in bytes.iter().enumerate().filter(|(_, byte)| **byte == b'%') {
        ensure!(
            bytes
                .get(index + 1..index + 3)
                .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit)),
            "invalid percent encoding"
        );
    }
    let value = value.replace('+', " ");
    Ok(percent_decode_str(&value)
        .decode_utf8()
        .context("query is not UTF-8")?
        .into_owned())
}

#[cfg(test)]
mod tests {
    use super::authority;

    #[test]
    fn authority_uses_the_browsers_default_port_spelling() -> anyhow::Result<()> {
        assert_eq!(authority("127.0.0.1:80".parse()?), "127.0.0.1");
        assert_eq!(authority("127.0.0.1:7878".parse()?), "127.0.0.1:7878");
        Ok(())
    }
}
