use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs::{self, File, Metadata, TryLockError};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use jiff::{ToSpan, civil::Date};
use sha1::{Digest, Sha1};

use crate::{notes, store};

use super::{Row, Totals};

pub(super) const MAX_ROW: usize = 1024 * 1024;
const FINGERPRINT_BYTES: usize = 256;
const FULL_FINGERPRINT: &str = "sha1:";

pub(super) fn create_directory(path: &Path) -> io::Result<()> {
    if !store::directory(path)? {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        if !store::directory(path)? {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("usage directory {} disappeared", path.display()),
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Month(Date);

impl Month {
    pub(super) fn of(day: Date) -> Self {
        Self(day.first_of_month())
    }

    pub(super) fn parse(name: &str) -> Option<Self> {
        let (year, month) = name.rsplit_once('-')?;
        let date = Date::new(year.parse().ok()?, month.parse().ok()?, 1).ok()?;
        let value = Self(date);
        (value.to_string() == name).then_some(value)
    }

    pub(super) fn path(self, directory: &Path) -> PathBuf {
        directory.join(format!("{self}.jsonl"))
    }

    pub(super) fn is_due(self, today: Date) -> bool {
        self.0
            .last_of_month()
            .checked_add(90.days())
            .is_ok_and(|expiry| expiry <= today)
    }
}

impl std::fmt::Display for Month {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}", self.0.year(), self.0.month())
    }
}

pub(super) fn months(directory: &Path) -> io::Result<BTreeSet<Month>> {
    let mut months = BTreeSet::new();
    for entry in fs::read_dir(directory)? {
        let name = entry?.file_name();
        if let Some(month) = name
            .to_str()
            .and_then(|name| name.strip_suffix(".jsonl"))
            .and_then(Month::parse)
        {
            months.insert(month);
        }
    }
    Ok(months)
}

pub(super) struct AppendGuard {
    _lock: File,
    directory: AppendDirectory,
    month: Month,
}

impl AppendGuard {
    pub(super) fn acquire(directory: &Path, month: Month) -> Result<Self> {
        let directory = AppendDirectory::open(directory)?;
        let lock = directory.private_file(&format!(".{month}.lock"), FileAccess::Create)?;
        lock.try_lock_shared()
            .map_err(io::Error::from)
            .with_context(|| format!("guarding usage month {month} for append"))?;
        ensure!(lock.metadata()?.len() == 0, "usage month {month} is sealed");
        Ok(Self {
            _lock: lock,
            directory,
            month,
        })
    }

    pub(super) fn append(&self, frame: &[u8]) -> io::Result<()> {
        let mut file = self
            .directory
            .private_file(&format!("{}.jsonl", self.month), FileAccess::Append)?;
        store::write_record(&mut file, frame)
    }
}

struct AppendDirectory {
    #[cfg(unix)]
    handle: File,
    #[cfg(windows)]
    _handle: File,
    #[cfg(windows)]
    path: PathBuf,
}

impl AppendDirectory {
    #[cfg(unix)]
    fn open(path: &Path) -> io::Result<Self> {
        use rustix::fs::{Mode, OFlags, open};

        let handle = open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        Ok(Self {
            handle: handle.into(),
        })
    }

    #[cfg(windows)]
    fn open(path: &Path) -> io::Result<Self> {
        Ok(Self {
            _handle: store::open_directory_for_child(path)?,
            path: path.into(),
        })
    }

    fn private_file(&self, name: &str, access: FileAccess) -> io::Result<File> {
        #[cfg(unix)]
        let file = open_file_at(&self.handle, Path::new(name), access)?;
        #[cfg(windows)]
        let file = open_file(&self.path.join(name), access)?;
        identity(&file)?;
        Ok(file)
    }
}

pub(super) struct ExpiryGuard(File);

impl ExpiryGuard {
    pub(super) fn try_acquire(directory: &Path, month: Month) -> Result<Option<Self>> {
        let file = open_month_lock(directory, month)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self(file))),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(error).context("locking usage month for expiry"),
        }
    }

    pub(super) fn prevent_appends(&mut self) -> io::Result<()> {
        if self.0.metadata()?.len() == 0 {
            self.0.write_all(b"1")?;
            self.0.sync_all()?;
        }
        Ok(())
    }
}

fn open_month_lock(directory: &Path, month: Month) -> io::Result<File> {
    let path = directory.join(format!(".{month}.lock"));
    private_file(&path, FileAccess::Create)
}

#[derive(Clone, Copy)]
pub(super) enum FileAccess {
    Read,
    Create,
    Append,
}

pub(super) fn private_file(path: &Path, access: FileAccess) -> io::Result<File> {
    let file = open_file(path, access)?;
    identity(&file)?;
    Ok(file)
}

#[cfg(unix)]
fn open_file(path: &Path, access: FileAccess) -> io::Result<File> {
    open_file_at(rustix::fs::CWD, path, access)
}

#[cfg(unix)]
fn open_file_at(
    directory: impl rustix::fd::AsFd,
    path: &Path,
    access: FileAccess,
) -> io::Result<File> {
    use rustix::fs::{self, Mode, OFlags};

    let access = match access {
        FileAccess::Read => OFlags::RDONLY,
        FileAccess::Create => OFlags::RDWR | OFlags::CREATE,
        FileAccess::Append => OFlags::WRONLY | OFlags::CREATE | OFlags::APPEND,
    };
    Ok(fs::openat(
        directory,
        path,
        access | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?
    .into())
}

#[cfg(windows)]
fn open_file(path: &Path, access: FileAccess) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    let mut options = File::options();
    match access {
        FileAccess::Read => options.read(true),
        FileAccess::Create => options.read(true).write(true).create(true).truncate(false),
        FileAccess::Append => options.append(true).create(true),
    };
    options
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(unix)]
fn identity(file: &File) -> io::Result<(String, Metadata)> {
    use std::os::unix::fs::MetadataExt;

    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(invalid_file());
    }
    Ok((format!("{}:{}", metadata.dev(), metadata.ino()), metadata))
}

#[cfg(windows)]
fn identity(file: &File) -> io::Result<(String, Metadata)> {
    const FILE_ATTRIBUTE_DIRECTORY: u64 = 0x10;
    const FILE_ATTRIBUTE_REPARSE_POINT: u64 = 0x400;

    let info = winapi_util::file::information(file)?;
    if info.file_attributes() & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || info.number_of_links() != 1
        || !winapi_util::file::typ(file)?.is_disk()
    {
        return Err(invalid_file());
    }
    Ok((
        format!("{}:{}", info.volume_serial_number(), info.file_index()),
        file.metadata()?,
    ))
}

fn invalid_file() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "expected an unredirected regular usage file with one link",
    )
}

#[derive(Debug)]
pub(super) struct Source {
    pub(super) generation: String,
    pub(super) offset: u64,
    pub(super) length: u64,
    pub(super) fingerprint: String,
}

pub(super) enum SourceState {
    Missing,
    Pending,
    Changed,
    Eof,
    Torn,
}

impl Source {
    fn matches(&self, file: &mut File, generation: &str, length: u64) -> io::Result<bool> {
        if self.generation != generation || self.length > length {
            return Ok(false);
        }
        let fingerprint = if self.has_full_fingerprint() {
            full_fingerprint(file, self.offset)?
        } else {
            legacy_fingerprint(file, self.offset)?
        };
        Ok(self.fingerprint == fingerprint)
    }

    pub(super) fn has_full_fingerprint(&self) -> bool {
        self.fingerprint.starts_with(FULL_FINGERPRINT)
    }

    pub(super) fn state(&self, path: &Path) -> io::Result<SourceState> {
        let mut file = match private_file(path, FileAccess::Read) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(SourceState::Missing);
            }
            result => result?,
        };
        let (generation, metadata) = identity(&file)?;
        let length = metadata.len();
        if !self.matches(&mut file, &generation, length)? {
            return Ok(SourceState::Changed);
        }
        if self.offset == length {
            Ok(SourceState::Eof)
        } else if self.length == length {
            Ok(SourceState::Torn)
        } else {
            Ok(SourceState::Pending)
        }
    }
}

pub(super) enum Scan {
    Missing,
    Gap,
    Complete { source: Source, totals: Totals },
}

pub(super) fn scan(path: &Path, month: Month, previous: Option<&Source>) -> Result<Scan> {
    let mut file = match private_file(path, FileAccess::Read) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Scan::Missing),
        result => result.with_context(|| format!("opening usage source {}", path.display()))?,
    };
    let (generation, metadata) = identity(&file)?;
    let length = metadata.len();
    let mut digest = Sha1::new();
    let mut sampled_prefix = false;
    let from = if let Some(previous) = previous {
        if previous.generation != generation || previous.length > length {
            return Ok(Scan::Gap);
        }
        if previous.has_full_fingerprint() {
            digest = digest_prefix(&mut file, previous.offset)?;
            if fingerprint(digest.clone())? != previous.fingerprint {
                return Ok(Scan::Gap);
            }
        } else {
            if legacy_fingerprint(&mut file, previous.offset)? != previous.fingerprint {
                return Ok(Scan::Gap);
            }
            sampled_prefix = previous.offset > (2 * FINGERPRINT_BYTES) as u64;
            if !sampled_prefix {
                digest = digest_prefix(&mut file, previous.offset)?;
            }
        }
        previous.offset
    } else {
        0
    };

    file.seek(SeekFrom::Start(from))?;
    let mut reader = BufReader::new((&mut file).take(length - from));
    let mut row = Vec::new();
    let mut totals = Totals::default();
    let mut offset = from;
    let mut complete_digest = digest.clone();
    while let Some(bytes) =
        store::read_complete_record(&mut reader, &mut row, MAX_ROW, |chunk| digest.update(chunk))?
    {
        offset += bytes;
        complete_digest.clone_from(&digest);
        if bytes == 1 {
            continue;
        }
        if let Ok(event) = serde_json::from_slice::<Row>(&row)
            && event.event.valid_ids()
            && let Ok((timestamp, day)) = event.when()
            && Month::of(day) == month
        {
            totals.add(event, timestamp, day);
        } else {
            totals.dropped += 1;
        }
    }

    let mut current = match private_file(path, FileAccess::Read) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Scan::Gap),
        result => result?,
    };
    let (current_generation, current_metadata) = identity(&current)?;
    if generation != current_generation || current_metadata.len() < length {
        return Ok(Scan::Gap);
    }
    if let Some(previous) = previous
        && !previous.has_full_fingerprint()
        && !previous.matches(&mut current, &generation, length)?
    {
        return Ok(Scan::Gap);
    }
    let fingerprint = if sampled_prefix {
        // A sampled legacy prefix cannot establish whole-file coverage at expiry.
        legacy_fingerprint(&mut current, offset)?
    } else {
        let scanned = fingerprint(complete_digest)?;
        if fingerprint(digest_prefix(&mut current, offset)?)? != scanned {
            return Ok(Scan::Gap);
        }
        scanned
    };

    Ok(Scan::Complete {
        source: Source {
            generation,
            offset,
            length,
            fingerprint,
        },
        totals,
    })
}

fn legacy_fingerprint(file: &mut File, offset: u64) -> io::Result<String> {
    let mut bytes = [0; FINGERPRINT_BYTES * 2];
    let length = usize::try_from(offset.min(FINGERPRINT_BYTES as u64)).map_err(io::Error::other)?;
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut bytes[..length])?;
    file.seek(SeekFrom::Start(
        offset - u64::try_from(length).map_err(io::Error::other)?,
    ))?;
    file.read_exact(&mut bytes[FINGERPRINT_BYTES..FINGERPRINT_BYTES + length])?;
    Ok(notes::sha1_hex(&bytes))
}

fn full_fingerprint(file: &mut File, offset: u64) -> io::Result<String> {
    fingerprint(digest_prefix(file, offset)?)
}

fn digest_prefix(file: &mut File, offset: u64) -> io::Result<Sha1> {
    file.seek(SeekFrom::Start(0))?;
    let mut remaining = offset;
    let mut digest = Sha1::new();
    let mut buffer = [0; 32 * 1024];
    while remaining != 0 {
        let count =
            usize::try_from(remaining.min(buffer.len() as u64)).map_err(io::Error::other)?;
        file.read_exact(&mut buffer[..count])?;
        digest.update(&buffer[..count]);
        remaining -= count as u64;
    }
    Ok(digest)
}

fn fingerprint(digest: Sha1) -> io::Result<String> {
    let mut text = String::with_capacity(FULL_FINGERPRINT.len() + 40);
    text.push_str(FULL_FINGERPRINT);
    for byte in digest.finalize() {
        write!(text, "{byte:02x}").map_err(io::Error::other)?;
    }
    Ok(text)
}
