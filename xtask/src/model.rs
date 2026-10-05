//! The typed result model every check produces.
//!
//! This is the part of the tooling that must not be compromised, so it is
//! worth being explicit about what it is for.
//!
//! A qualification harness fails most dangerously when it reports a verdict
//! its own plumbing invented rather than one a command actually produced. The
//! R09 ledger records several instances of exactly that: a repetition gate
//! that counted the substring `agreed=8`, a causal gate that reported RED when
//! a metric merely differed from zero, and a clippy check that printed PASS
//! because a pipeline inherited `cargo`'s exit status through `pipefail`. In
//! every case a human reading the output had no way to tell that the green
//! result was not a result at all.
//!
//! So the rules encoded here are narrow and deliberate:
//!
//! - A disposition comes from a child process's exit status or from a
//!   structural fact. It never comes from matching text in a log.
//! - Infrastructure breakage is a different disposition from a product
//!   failure, because "the harness is broken" and "the code is wrong" lead to
//!   opposite responses.
//! - Every check records the exact argv it ran. Not a shell string: a command
//!   is a program plus arguments, the same discipline the product's own
//!   launch capability applies.

use std::{fmt, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

/// How a check ended.
///
/// The three failure modes are separate types rather than a boolean because
/// the first thing anyone does with a red qualification is ask *whose* fault
/// it is, and collapsing these into `failed` makes that question unanswerable
/// from the report alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Disposition {
    /// The check ran and the thing it checks is healthy.
    Pass,
    /// The check ran and the thing it checks is not healthy. This is a
    /// statement about the subject under test.
    CheckFail,
    /// The check could not be run, or could not be evaluated. The subject was
    /// never tested, so this says nothing about it.
    HarnessError,
    /// The check ran but the environment could not support it - a missing
    /// system library, no network, no writable directory. Also says nothing
    /// about the subject.
    EnvironmentError,
}

impl Disposition {
    /// Whether this disposition means the gate may proceed.
    pub fn is_success(self) -> bool {
        matches!(self, Self::Pass)
    }

    /// Whether this disposition means the subject failed, as opposed to never
    /// having been tested.
    ///
    /// Used by the self-tests to read the distinction back; not called by the
    /// binary itself, which only needs the aggregate.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_subject_failure(self) -> bool {
        matches!(self, Self::CheckFail)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::CheckFail => "CHECK_FAIL",
            Self::HarnessError => "HARNESS_ERROR",
            Self::EnvironmentError => "ENVIRONMENT_ERROR",
        }
    }
}

impl fmt::Display for Disposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What kind of check this is.
///
/// The distinction is not cosmetic. It decides which checks CI is allowed to
/// run at all. A repository-hermetic check is one whose answer does not depend
/// on the machine; a host check is about *this* workstation's installed
/// state; a human check cannot be automated at all and must never appear in a
/// machine gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckClass {
    /// Answerable anywhere: tests, clippy, format, builds, source invariants.
    /// Suitable for local use and for CI.
    RepositoryHermetic,
    /// About one installed workstation: installed binary digests, systemd
    /// user units, compositor state, live accessibility behaviour, privilege
    /// elevation, real launches and real persistence.
    ///
    /// These must never become CI gates. A GitHub runner is not the Gnosix
    /// workstation, and a check about `/usr/libexec` would pass on a runner
    /// for the wrong reason: the file simply would not be there.
    HostLive,
    /// A judgement only a person can make: perceptual review, screen-reader
    /// usability, publishability, release acceptance.
    HumanAcceptance,
}

impl CheckClass {
    /// Whether this class belongs in a repository-hermetic CI profile.
    pub fn runs_in_ci(self) -> bool {
        matches!(self, Self::RepositoryHermetic)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::RepositoryHermetic => "A_REPOSITORY_HERMETIC",
            Self::HostLive => "B_HOST_LIVE",
            Self::HumanAcceptance => "C_HUMAN_ACCEPTANCE",
        }
    }
}

/// The observed facts about a check that ran and failed.
#[derive(Clone, Debug)]
pub struct Failure {
    pub exit_status: Option<i32>,
    pub duration: Duration,
    pub log_path: PathBuf,
    pub log_sha256: String,
    pub excerpt: Option<String>,
}

/// The outcome of running one check.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckResult {
    /// Stable identifier, used in reports and in `--only` selection.
    pub name: String,
    pub class: CheckClass,
    /// The exact program and arguments run. Never a shell string.
    pub argv: Vec<String>,
    /// The child's exit status, absent when no child was ever started.
    pub exit_status: Option<i32>,
    pub disposition: Disposition,
    pub duration_ms: u64,
    /// Where the full log for this check was written.
    pub log_path: PathBuf,
    /// Digest of that log, so retained evidence is tamper-evident.
    pub log_sha256: Option<String>,
    /// A bounded excerpt for the console. Full logs stay in their files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_excerpt: Option<String>,
}

impl CheckResult {
    /// A check that passed.
    pub fn passed(
        name: impl Into<String>,
        class: CheckClass,
        argv: Vec<String>,
        duration: Duration,
        log_path: PathBuf,
        log_sha256: String,
    ) -> Self {
        Self {
            name: name.into(),
            class,
            argv,
            exit_status: Some(0),
            disposition: Disposition::Pass,
            duration_ms: duration.as_millis() as u64,
            log_path,
            log_sha256: Some(log_sha256),
            diagnostic_excerpt: None,
        }
    }

    /// A check that ran and found the subject unhealthy.
    pub fn subject_failure(
        name: impl Into<String>,
        class: CheckClass,
        argv: Vec<String>,
        failure: Failure,
    ) -> Self {
        Self {
            name: name.into(),
            class,
            argv,
            exit_status: failure.exit_status,
            disposition: Disposition::CheckFail,
            duration_ms: failure.duration.as_millis() as u64,
            log_path: failure.log_path,
            log_sha256: Some(failure.log_sha256),
            diagnostic_excerpt: failure.excerpt,
        }
    }

    /// A check that never produced a verdict about the subject.
    ///
    /// `exit_status` is `None` because no child ran, or the child was killed.
    /// Recording a synthetic status here would be the same class of error as
    /// inferring a verdict from log text.
    pub fn harness_error(
        name: impl Into<String>,
        class: CheckClass,
        argv: Vec<String>,
        duration: Duration,
        log_path: PathBuf,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            class,
            argv,
            exit_status: None,
            disposition: Disposition::HarnessError,
            duration_ms: duration.as_millis() as u64,
            log_path,
            log_sha256: None,
            diagnostic_excerpt: Some(detail.into()),
        }
    }

    pub fn is_success(&self) -> bool {
        self.disposition.is_success()
    }
}

/// The aggregate result of a verification run.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerificationReport {
    pub schema: String,
    /// Which profile produced this: `verify` or `ci`.
    pub profile: String,
    /// Git commit, when one could be read.
    pub git_commit: Option<String>,
    /// Whether the working tree had uncommitted changes.
    ///
    /// Recorded rather than assumed, because a dirty qualification and a
    /// clean one are not the same claim and a closure-grade record has to say
    /// which it is.
    pub dirty: Option<bool>,
    pub results: Vec<CheckResult>,
    /// The overall disposition: `Pass` only when every required check passed.
    pub overall: Disposition,
    pub started_unix_ms: u128,
    pub duration_ms: u64,
}

impl VerificationReport {
    /// Combine check results into an overall disposition.
    ///
    /// Deliberately the *worst* disposition rather than the first failure, so
    /// that one broken Clippy target does not hide the diagnostics of the
    /// five after it. Reporting only the first failure is how a green-looking
    /// report hides two real problems.
    pub fn aggregate(results: Vec<CheckResult>) -> Disposition {
        if results.is_empty() {
            // No checks ran. That is never a pass: an empty gate that reports
            // success is the most dangerous state this tool could be in.
            return Disposition::HarnessError;
        }
        let mut overall = Disposition::Pass;
        for result in &results {
            let rank = match result.disposition {
                Disposition::Pass => 0,
                Disposition::CheckFail => 2,
                Disposition::EnvironmentError => 3,
                Disposition::HarnessError => 4,
            };
            let current = match overall {
                Disposition::Pass => 0,
                Disposition::CheckFail => 2,
                Disposition::EnvironmentError => 3,
                Disposition::HarnessError => 4,
            };
            if rank > current {
                overall = result.disposition;
            }
        }
        overall
    }

    pub fn is_success(&self) -> bool {
        self.overall.is_success()
    }

    pub fn failures(&self) -> Vec<&CheckResult> {
        self.results.iter().filter(|r| !r.is_success()).collect()
    }
}
