use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

use super::process::run_with_input;
use super::{GitRepo, discover, git_cmd, run, strip_lf};
use crate::config::KbRoot;
use crate::journal;

const LIMIT: usize = 2048;
const FORMAT: &str = "--format=%x00%H%x00%P%x00%cI%x00%s%x00%(trailers:key=Knowbox-Client,key=Knowbox-Host,key=Knowbox-Session,only,unfold)%x00";
const DIFF_ARGS: [&str; 8] = [
    "--no-ext-diff",
    "--no-textconv",
    "--no-color",
    "--no-relative",
    "--ignore-submodules=none",
    "--no-abbrev",
    "-M",
    "-l1000",
];

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Commit {
    pub(crate) oid: String,
    pub(crate) parents: Vec<String>,
    pub(crate) time: String,
    pub(crate) subject: String,
    pub(crate) client: Option<String>,
    pub(crate) host: Option<String>,
    pub(crate) session: Option<String>,
}

#[derive(Debug)]
pub(crate) struct Image {
    pub(crate) path: String,
    pub(crate) oid: String,
}

#[derive(Debug)]
pub(crate) struct Change {
    pub(crate) before: Option<Image>,
    pub(crate) after: Option<Image>,
}

impl Change {
    pub(crate) fn note(&self) -> bool {
        self.images().any(|image| is_note(&image.path))
    }

    pub(crate) fn images(&self) -> impl Iterator<Item = &Image> {
        self.before.iter().chain(&self.after)
    }
}

pub(crate) fn is_note(path: &str) -> bool {
    path.strip_prefix("notes/")
        .is_some_and(|name| !name.contains('/') && name.ends_with(".md"))
}

pub(crate) struct Revision {
    pub(crate) commit: Commit,
    pub(crate) changes: Vec<Change>,
}

pub(crate) struct History<'a> {
    pub(crate) repo: GitRepo<'a>,
    pub(crate) revisions: Vec<Revision>,
    pub(crate) truncated: bool,
    pub(crate) shallow: bool,
}

#[derive(Default)]
pub(crate) struct Blobs {
    file: Option<File>,
    offsets: BTreeMap<String, (u64, usize)>,
}

impl Blobs {
    pub(crate) fn get(&mut self, oid: &str) -> Result<Vec<u8>> {
        let (offset, size) = self.offsets.get(oid).copied().context("missing Git blob")?;
        let file = self.file.as_mut().context("missing Git blob storage")?;
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0; size];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
}

pub(super) fn read_cmd(repo: &GitRepo<'_>) -> Command {
    let mut cmd = git_cmd(repo.root);
    cmd.args([
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.untrackedCache=false",
    ])
    .env("GIT_ALLOW_PROTOCOL", "")
    .env("GIT_NO_REPLACE_OBJECTS", "1")
    .env("GIT_NO_LAZY_FETCH", "1");
    cmd
}

pub(super) fn head(repo: &GitRepo<'_>) -> Result<Option<String>> {
    let data = run(
        read_cmd(repo).args(["rev-parse", "--verify", "-q", "HEAD"]),
        "rev-parse",
    )?
    .optional()?;
    data.map(|data| oid(strip_lf(&data))).transpose()
}

pub(super) fn commit(repo: &GitRepo<'_>, id: &str) -> Result<Commit> {
    let data = run(
        read_cmd(repo).args([
            "log",
            "-1",
            "-z",
            FORMAT,
            "--no-notes",
            "--no-decorate",
            "--no-show-signature",
            id,
            "--",
        ]),
        "log",
    )?
    .checked()?;
    let mut records = data.split(|&b| b == 0);
    ensure!(records.next() == Some(b""), "missing Git commit header");
    parse_commit(&mut records)
}

pub(crate) fn read(root: &KbRoot) -> Result<Option<History<'_>>> {
    let Some(repo) = discover(root)? else {
        return Ok(None);
    };
    let Some(head) = head(&repo)? else {
        return Ok(Some(History {
            repo,
            revisions: Vec::new(),
            truncated: false,
            shallow: false,
        }));
    };
    let limit = format!("--max-count={}", LIMIT + 1);
    let data = run(
        read_cmd(&repo)
            .args([
                "log",
                &limit,
                "--topo-order",
                "--full-history",
                "--root",
                "-z",
                "--raw",
                "--diff-merges=off",
                "--no-notes",
                "--no-decorate",
                "--no-show-signature",
                FORMAT,
            ])
            .args(DIFF_ARGS)
            .args([&head, "--"]),
        "log",
    )?
    .checked()?;
    let mut revisions = parse_revisions(&data)?;
    let shallow = run(
        read_cmd(&repo).args(["rev-parse", "--is-shallow-repository"]),
        "rev-parse",
    )?
    .checked()?;
    let shallow = strip_lf(&shallow) == b"true";
    let truncated = revisions.len() > LIMIT || shallow;
    revisions.truncate(LIMIT);
    revisions.reverse();
    for revision in &mut revisions {
        if shallow && revision.commit.parents.is_empty() {
            revision.changes.clear();
            continue;
        }
        if let [parent, _, ..] = revision.commit.parents.as_slice() {
            let data = run(
                read_cmd(&repo)
                    .args(["diff-tree", "--no-commit-id", "-r", "--raw", "-z"])
                    .args(DIFF_ARGS)
                    .args([parent, &revision.commit.oid, "--"]),
                "diff-tree",
            )?
            .checked()?;
            let mut records = data.split(|&b| b == 0);
            while let Some(header) = records.next().filter(|header| !header.is_empty()) {
                record(&mut revision.changes, header, &mut records)?;
            }
        }
    }
    Ok(Some(History {
        repo,
        revisions,
        truncated,
        shallow,
    }))
}

impl History<'_> {
    pub(crate) fn blobs(&self) -> Result<Blobs> {
        let ids: BTreeSet<_> = self
            .revisions
            .iter()
            .flat_map(|revision| &revision.changes)
            .flat_map(Change::images)
            .filter(|image| image.path == "log.md" || is_note(&image.path))
            .map(|image| image.oid.as_str())
            .collect();
        if ids.is_empty() {
            return Ok(Blobs::default());
        }
        let mut input = tempfile::tempfile()?;
        for id in &ids {
            writeln!(input, "{id}")?;
        }
        input.rewind()?;
        let file = run_with_input(
            read_cmd(&self.repo).args(["cat-file", "--batch"]),
            "cat-file",
            input.into(),
        )?
        .checked_file()?;
        let total = file.metadata()?.len();
        ensure!(
            total <= 512 * 1024 * 1024,
            "Git history blobs exceeded 512 MiB"
        );
        let mut reader = BufReader::new(file);
        let mut offsets = BTreeMap::new();
        let mut line = String::new();
        for expected in ids {
            line.clear();
            reader.read_line(&mut line)?;
            let mut fields = line.split_ascii_whitespace();
            ensure!(
                fields.next() == Some(expected) && fields.next() == Some("blob"),
                "Git blob unavailable: {expected}"
            );
            let size: usize = fields.next().context("missing Git blob size")?.parse()?;
            ensure!(
                size <= 64 * 1024 * 1024,
                "Git blob {expected} exceeded 64 MiB"
            );
            let offset = reader.stream_position()?;
            let end = offset
                .checked_add(u64::try_from(size)?)
                .context("invalid Git blob size")?;
            ensure!(end < total, "truncated Git blob {expected}");
            reader.seek(SeekFrom::Start(end))?;
            let mut separator = [0];
            reader.read_exact(&mut separator)?;
            ensure!(separator == *b"\n", "missing Git blob separator");
            offsets.insert(expected.to_owned(), (offset, size));
        }
        ensure!(
            reader.stream_position()? == total,
            "unexpected data after Git blobs"
        );
        Ok(Blobs {
            file: Some(reader.into_inner()),
            offsets,
        })
    }

    pub(crate) fn patch(&self, commit: &Commit, change: &Change) -> Result<String> {
        let mut cmd = read_cmd(&self.repo);
        cmd.args([
            "diff-tree",
            "--root",
            "--no-commit-id",
            "-r",
            "--patch",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--diff-algorithm=myers",
        ])
        .args(DIFF_ARGS);
        if let Some(parent) = commit.parents.first() {
            cmd.arg(parent);
        }
        cmd.args([&commit.oid, "--"]);
        for image in change.images() {
            cmd.arg(&image.path);
        }
        let data = run(&mut cmd, "diff-tree")?.checked()?;
        Ok(String::from_utf8_lossy(&data).into_owned())
    }
}

fn oid(bytes: &[u8]) -> Result<String> {
    let oid = std::str::from_utf8(bytes)?;
    ensure!(
        matches!(oid.len(), 40 | 64)
            && oid
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f')),
        "invalid Git object ID"
    );
    Ok(oid.to_owned())
}

fn token<'a>(records: &mut impl Iterator<Item = &'a [u8]>) -> Result<&'a [u8]> {
    records.next().context("truncated Git record")
}

fn parse_commit<'a>(records: &mut impl Iterator<Item = &'a [u8]>) -> Result<Commit> {
    let id = oid(token(records)?)?;
    let parents = token(records)?
        .split(|&b| b == b' ')
        .filter(|p| !p.is_empty())
        .map(oid)
        .collect::<Result<Vec<_>>>()?;
    let time = std::str::from_utf8(token(records)?)?.to_owned();
    let subject = String::from_utf8_lossy(token(records)?).into_owned();
    let trailers = String::from_utf8_lossy(token(records)?);
    let trailer = |key: &str| {
        let mut values = trailers.lines().filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            if !name.eq_ignore_ascii_case(key) {
                return None;
            }
            journal::label(name, value).ok()
        });
        let value = values.next()?;
        values.next().is_none().then(|| value.to_owned())
    };
    Ok(Commit {
        oid: id,
        parents,
        time,
        subject,
        client: trailer("Knowbox-Client"),
        host: trailer("Knowbox-Host"),
        session: trailer("Knowbox-Session"),
    })
}

fn parse_revisions(data: &[u8]) -> Result<Vec<Revision>> {
    let mut records = data.split(|&byte| byte == 0).peekable();
    let mut revisions: Vec<Revision> = Vec::new();
    while let Some(next) = records.next() {
        if next.is_empty() {
            if records
                .peek()
                .is_some_and(|next| matches!(next.len(), 40 | 64))
            {
                revisions.push(Revision {
                    commit: parse_commit(&mut records)?,
                    changes: Vec::new(),
                });
            }
            continue;
        }
        let revision = revisions.last_mut().context("missing Git commit header")?;
        record(&mut revision.changes, next, &mut records)?;
    }
    Ok(revisions)
}

fn record<'a>(
    changes: &mut Vec<Change>,
    header: &[u8],
    records: &mut impl Iterator<Item = &'a [u8]>,
) -> Result<()> {
    let header = std::str::from_utf8(header)?.trim_start_matches('\n');
    let fields: Vec<_> = header
        .strip_prefix(':')
        .context("invalid Git raw diff header")?
        .split_ascii_whitespace()
        .collect();
    let [old_mode, new_mode, old, new, status] = fields.as_slice() else {
        bail!("invalid Git raw diff fields");
    };
    let old_path = token(records)?;
    let new_path = if status.starts_with(['R', 'C']) {
        token(records)?
    } else {
        old_path
    };
    let image = |mode: &str, id: &str, path: &[u8]| -> Result<Option<Image>> {
        let Ok(path) = std::str::from_utf8(path) else {
            return Ok(None);
        };
        if !matches!(mode, "100644" | "100755") {
            return Ok(None);
        }
        Ok(Some(Image {
            path: path.to_owned(),
            oid: oid(id.as_bytes())?,
        }))
    };
    let relevant =
        |path| std::str::from_utf8(path).is_ok_and(|path| path == "log.md" || is_note(path));
    if !relevant(old_path) && !relevant(new_path) {
        return Ok(());
    }
    let before = image(old_mode, old, old_path)?;
    let after = image(new_mode, new, new_path)?;
    if before.is_some() || after.is_some() {
        changes.push(Change { before, after });
    }
    Ok(())
}
