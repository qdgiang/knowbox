use std::io;
use std::time::{Duration, SystemTime};

use serde::Serialize;

use crate::{config::KbRoot, store::file_metadata};

use super::Findings;

const SECONDS_PER_DAY: u64 = 86_400;

struct Service {
    name: &'static str,
    filename: &'static str,
    max_days: u64,
    missing_help: &'static str,
    stale_help: &'static str,
}

const SERVICES: [Service; 2] = [
    Service {
        name: "janitor",
        filename: ".kb-heartbeat",
        max_days: 8,
        missing_help: "weekly cron dead? re-run your KB maintenance script",
        stale_help: "weekly cron dead? re-run your KB maintenance script",
    },
    Service {
        name: "sweep",
        filename: ".kb-sweep-heartbeat",
        max_days: 2,
        missing_help: "4-hourly sweep cron dead? check the crontab and .sweep-cron.log",
        stale_help: "sweep cron dead? check .sweep-cron.log",
    },
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum State {
    Fresh,
    Stale,
    Missing,
    Unavailable,
}

#[derive(Debug, Serialize)]
pub(crate) struct Heartbeat {
    service: &'static str,
    state: State,
    age_days: Option<u64>,
}

impl Service {
    fn read(&self, root: &KbRoot, now: SystemTime) -> io::Result<Heartbeat> {
        let meta = file_metadata(&root.path().join(self.filename))?;
        let (state, age_days) = if let Some(meta) = meta {
            let age = now
                .duration_since(meta.modified()?)
                .unwrap_or(Duration::ZERO);
            let state = if age > Duration::from_secs(self.max_days * SECONDS_PER_DAY) {
                State::Stale
            } else {
                State::Fresh
            };
            (state, Some(age.as_secs() / SECONDS_PER_DAY))
        } else {
            (State::Missing, None)
        };
        Ok(Heartbeat {
            service: self.name,
            state,
            age_days,
        })
    }
}

/// Inspect both scheduled services against the same modification-time boundaries used by lint.
pub(crate) fn heartbeats(root: &KbRoot, now: SystemTime) -> Vec<Heartbeat> {
    SERVICES
        .iter()
        .map(|service| {
            service.read(root, now).unwrap_or(Heartbeat {
                service: service.name,
                state: State::Unavailable,
                age_days: None,
            })
        })
        .collect()
}

impl Findings {
    pub(super) fn check_heartbeats(&mut self, root: &KbRoot, now: SystemTime) {
        for service in &SERVICES {
            let name = service.name;
            let location = service.filename.into();
            match service.read(root, now) {
                Ok(report) => match report.state {
                    State::Missing if !cfg!(windows) => self.error(
                        "heartbeat_missing",
                        location,
                        format!("{name} heartbeat missing — {}", service.missing_help),
                    ),
                    State::Stale => self.error(
                        "heartbeat_stale",
                        location,
                        format!(
                            "{name} heartbeat stale (older than {}d) — {}",
                            service.max_days, service.stale_help
                        ),
                    ),
                    State::Fresh | State::Missing | State::Unavailable => {}
                },
                Err(error) => self.error(
                    "heartbeat_unreadable",
                    location,
                    format!("{name} heartbeat unreadable: {error}"),
                ),
            }
        }
    }
}
