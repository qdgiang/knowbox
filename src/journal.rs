use std::{env, fs};

use anyhow::{Result, ensure};
use jiff::{Timestamp, Zoned, civil::Date};
use serde::Serialize;

#[derive(Debug)]
pub(crate) struct Actor {
    client: String,
    host: String,
    session: Option<String>,
}

impl Actor {
    pub(crate) fn resolve(explicit_client: Option<&str>, default_client: &str) -> Result<Self> {
        let client = match explicit_client {
            Some(value) => value.to_owned(),
            None => setting("KNOWBOX_CLIENT")?.unwrap_or_else(|| default_client.to_owned()),
        };
        let host = setting("KNOWBOX_HOST")?.unwrap_or_else(hostname);
        let session = match setting("KNOWBOX_SESSION")? {
            Some(session) => Some(session),
            None => [
                "CODEX_THREAD_ID",
                "CODEX_SESSION_ID",
                "CLAUDE_SESSION_ID",
                "OPENCODE_SESSION_ID",
            ]
            .into_iter()
            .find_map(automatic_label),
        };
        Self::new(&client, &host, session.as_deref())
    }

    pub(crate) fn new(client: &str, host: &str, session: Option<&str>) -> Result<Self> {
        Ok(Self {
            client: label("client", client)?.to_owned(),
            host: label("host", host)?.to_owned(),
            session: session
                .map(|value| label("session", value).map(str::to_owned))
                .transpose()?,
        })
    }

    pub(crate) fn manual(&self) -> Self {
        Self {
            client: "manual".into(),
            host: self.host.clone(),
            session: None,
        }
    }

    pub(crate) fn client(&self) -> &str {
        &self.client
    }

    pub(crate) fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    pub(crate) fn commit_message(&self, subject: &str, reasons: &[&str]) -> String {
        let mut message = format!(
            "{subject}\n\nKnowbox-Client: {}\nKnowbox-Host: {}",
            self.client, self.host
        );
        if let Some(session) = self.session() {
            message.push_str("\nKnowbox-Session: ");
            message.push_str(session);
        }
        for reason in reasons {
            message.push_str("\nKnowbox-Supersede-Reason: ");
            message.push_str(reason);
        }
        message
    }
}

fn setting(key: &str) -> Result<Option<String>> {
    env::var_os(key)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| anyhow::anyhow!("{key} must be UTF-8"))
        })
        .transpose()
        .map(|value| value.filter(|value| !value.trim().is_empty()))
}

pub(crate) fn label<'a>(field: &str, value: &'a str) -> Result<&'a str> {
    let value = value.trim_matches(' ');
    ensure!(
        !value.is_empty()
            && value.len() <= 256
            && value != "—"
            && !value.contains('|')
            && !value.chars().any(char::is_control),
        "{field} must be a nonempty label of at most 256 bytes without pipes or controls"
    );
    Ok(value)
}

fn automatic_label(key: &str) -> Option<String> {
    let value = env::var(key).ok()?;
    label(key, value.trim()).ok().map(str::to_owned)
}

fn hostname() -> String {
    for key in ["HOSTNAME", "COMPUTERNAME"] {
        if let Some(value) = automatic_label(key) {
            return value;
        }
    }
    for path in ["/proc/sys/kernel/hostname", "/etc/hostname"] {
        if let Ok(value) = fs::read_to_string(path)
            && let Ok(host) = label("host", value.trim())
        {
            return host.to_owned();
        }
    }
    if let Ok(output) = std::process::Command::new("hostname").output()
        && output.status.success()
        && let Ok(value) = std::str::from_utf8(&output.stdout)
        && let Ok(host) = label("host", value.trim())
    {
        return host.to_owned();
    }
    "unknown".into()
}

#[derive(Debug)]
pub(crate) struct Stamp<'a> {
    pub(crate) actor: &'a Actor,
    pub(crate) time: Zoned,
}

impl<'a> Stamp<'a> {
    pub(crate) fn now(actor: &'a Actor) -> Self {
        Self {
            actor,
            time: Zoned::now(),
        }
    }

    pub(crate) fn date(&self) -> Date {
        self.time.date()
    }

    pub(crate) fn heading(&self, action: &str, id: &str, detail: &str) -> Result<String> {
        for (field, value) in [("action", action), ("id", id), ("detail", detail)] {
            ensure!(
                !value.contains(['\r', '\n', '\0']),
                "{field} must be a single line without NUL"
            );
        }
        ensure!(
            !action.contains('|') && !id.contains('|'),
            "journal action and id cannot contain a pipe"
        );
        let time = self.time.strftime("%Y-%m-%dT%H:%M:%S%:z");
        let Actor {
            client,
            host,
            session,
        } = self.actor;
        let session = session.as_deref().unwrap_or("—");
        Ok(format!(
            "## [{time}] {action} | {id} | client={client} | host={host} | session={session} | {detail}"
        ))
    }
}

pub(crate) fn iso_day(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(i, byte)| {
            if i == 4 || i == 7 {
                *byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
}

pub(crate) fn local_day(stamp: &str) -> Option<&str> {
    let day = stamp.get(..10).filter(|day| iso_day(day))?;
    let suffix = stamp.get(10..)?;
    if suffix.is_empty() || (suffix.starts_with('T') && stamp.parse::<Timestamp>().is_ok()) {
        Some(day)
    } else {
        None
    }
}

pub(crate) fn split_heading(line: &str) -> Option<(&str, &str)> {
    line.strip_prefix("## [")?.split_once(']')
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Heading {
    pub(crate) time: String,
    pub(crate) action: String,
    pub(crate) id: Option<String>,
    pub(crate) client: Option<String>,
    pub(crate) host: Option<String>,
    pub(crate) session: Option<String>,
    pub(crate) detail: String,
}

pub(crate) fn heading(line: &str) -> Option<Heading> {
    let (time, rest) = split_heading(line)?;
    local_day(time)?.parse::<Date>().ok()?;
    let (action, rest) = rest.strip_prefix(' ')?.split_once(" | ")?;
    let (id, detail) = rest.split_once(" | ").unwrap_or((rest, ""));
    if action.is_empty()
        || !action
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
    {
        return None;
    }
    let id = match id {
        "—" | "-" | "" => None,
        id if crate::notes::is_id(id) => Some(id.to_owned()),
        _ => return None,
    };
    let mut heading = Heading {
        time: time.to_owned(),
        action: action.to_owned(),
        id,
        client: None,
        host: None,
        session: None,
        detail: detail.to_owned(),
    };
    let mut fields = detail.splitn(4, " | ");
    if let Some(client) = fields
        .next()
        .and_then(|field| field.strip_prefix("client="))
        && let Ok(client) = label("client", client)
        && let Some(host) = fields.next().and_then(|field| field.strip_prefix("host="))
        && let Ok(host) = label("host", host)
        && let Some(session) = fields
            .next()
            .and_then(|field| field.strip_prefix("session="))
        && let Some(detail) = fields.next()
    {
        let session = match session {
            "" | "—" => None,
            value => match label("session", value) {
                Ok(session) => Some(session.to_owned()),
                Err(_) => return Some(heading),
            },
        };
        heading.client = Some(client.to_owned());
        heading.host = Some(host.to_owned());
        heading.session = session;
        heading.detail = detail.to_owned();
    }
    Some(heading)
}

pub(crate) fn supersede_reason(line: &str) -> Option<&str> {
    let (stamp, rest) = split_heading(line)?;
    local_day(stamp)?;
    if !stamp.contains('T') {
        return None;
    }
    let mut fields = rest.strip_prefix(' ')?.splitn(6, " | ");
    if fields.next()? != "supersede" || !crate::notes::is_id(fields.next()?) {
        return None;
    }
    fields.next()?.strip_prefix("client=")?;
    fields.next()?.strip_prefix("host=")?;
    fields.next()?.strip_prefix("session=")?;
    let (successor, reason) = fields
        .next()?
        .strip_prefix("superseded by ")?
        .split_once(" | reason=")?;
    (crate::notes::is_id(successor)
        && !reason.trim().is_empty()
        && !reason.contains(['\r', '\n', '\0']))
    .then_some(reason)
}

#[cfg(test)]
pub(crate) fn test_stamp() -> Stamp<'static> {
    use std::sync::LazyLock;

    static ACTOR: LazyLock<Actor> =
        LazyLock::new(|| Actor::new("test", "test-host", Some("test-session")).unwrap());
    Stamp {
        actor: &ACTOR,
        time: "2026-11-18T09:10:11+07:00[+07:00]".parse().unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;

    use super::*;

    #[test]
    fn date_only_heading_preserves_optional_actor_and_evidence() -> Result<()> {
        let event = heading(
            "## [2026-11-18] verify | kb-0042 | client=opencode | host=fedora | session=s123 | first | second",
        ).context("valid date-only heading")?;
        assert_eq!(event.time, "2026-11-18");
        assert_eq!(event.client.as_deref(), Some("opencode"));
        assert_eq!(event.host.as_deref(), Some("fedora"));
        assert_eq!(event.session.as_deref(), Some("s123"));
        assert_eq!(event.detail, "first | second");
        assert_eq!(
            heading("## [2026-11-18] create | kb-0042")
                .context("heading without detail")?
                .detail,
            "",
        );
        Ok(())
    }

    #[test]
    fn journal_and_git_preserve_detail_and_local_day() -> Result<()> {
        let actor = Actor::new("codex", "fedora-server", Some("session-1"))?;
        let stamp = Stamp {
            actor: &actor,
            time: "2026-11-22T00:03:04+07:00[+07:00]"
                .parse()
                .context("test timestamp")?,
        };
        assert_eq!(stamp.date(), Date::new(2026, 11, 22)?);
        assert_eq!(
            stamp.heading("verify", "kb-0042", "linked source | line 2")?,
            "## [2026-11-22T00:03:04+07:00] verify | kb-0042 | client=codex | host=fedora-server | session=session-1 | linked source | line 2"
        );
        assert_eq!(local_day("2026-11-22T00:03:04+07:00"), Some("2026-11-22"));
        assert_eq!(local_day("2026-11-21"), Some("2026-11-21"));
        assert_eq!(local_day("2026-11-22T00:03:04"), None);
        assert_eq!(
            actor.commit_message("subject", &["reason | reference"]),
            "subject\n\nKnowbox-Client: codex\nKnowbox-Host: fedora-server\nKnowbox-Session: session-1\nKnowbox-Supersede-Reason: reason | reference"
        );
        Ok(())
    }

    #[test]
    fn actor_rejects_trailer_and_field_delimiters() {
        for client in ["", "   ", "web | host=other", "agent\nKnowbox-Host: forged"] {
            assert_eq!(
                Actor::new(client, "host", None).unwrap_err().to_string(),
                "client must be a nonempty label of at most 256 bytes without pipes or controls"
            );
        }
    }
}
