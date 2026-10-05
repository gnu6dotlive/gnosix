//! Process execution, with the discipline the product's own launch capability
//! applies.
//!
//! Every command is spawned as a program plus typed arguments. There is no
//! `sh -c` anywhere in this crate, and no command is ever assembled into a
//! string for a shell to re-parse. That is not stylistic: a shell string is
//! where quoting bugs, injection and "it worked because of how the words
//! fell" all live, and this crate's job is to produce verdicts people trust.
//!
//! The other thing this module guarantees is that **a verdict comes from the
//! child**. `Command::status` gives a real exit status or a spawn error. There
//! is no path here that pipes a child's output into a filter and infers
//! success from what came out, because that is exactly how the earlier
//! Phase 4 matrix ended up printing PASS next to visible clippy errors.

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::model::{CheckClass, CheckResult, Disposition, Failure};

/// How a spawned command ended.
pub enum SpawnOutcome {
    /// The child ran and exited with this status.
    Exited(i32),
    /// The child ran and was terminated by a signal, with no exit status.
    Signalled,
    /// The child never started.
    FailedToStart(String),
}

/// Run one command, capturing all output to a log file.
///
/// Returns the typed outcome. Never interprets the output.
pub fn run_logged(
    name: &str,
    class: CheckClass,
    argv: &[String],
    log_path: &Path,
    cwd: &Path,
) -> CheckResult {
    let started = std::time::Instant::now();

    if let Some(parent) = log_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return CheckResult::harness_error(
            name,
            class,
            argv.to_vec(),
            started.elapsed(),
            log_path.to_path_buf(),
            format!("could not create log directory: {error}"),
        );
    }

    let log_file = match File::create(log_path) {
        Ok(file) => file,
        Err(error) => {
            return CheckResult::harness_error(
                name,
                class,
                argv.to_vec(),
                started.elapsed(),
                log_path.to_path_buf(),
                format!("could not create log file: {error}"),
            );
        }
    };
    let mut log_file = log_file;

    let program = argv.first().map(String::as_str).unwrap_or_default();
    if program.is_empty() {
        return CheckResult::harness_error(
            name,
            class,
            argv.to_vec(),
            started.elapsed(),
            log_path.to_path_buf(),
            "empty command",
        );
    }

    let Ok(stdout) = try_clone(&log_file) else {
        return CheckResult::harness_error(
            name,
            class,
            argv.to_vec(),
            started.elapsed(),
            log_path.to_path_buf(),
            "could not duplicate the log handle for stdout",
        );
    };
    let Ok(stderr) = try_clone(&log_file) else {
        return CheckResult::harness_error(
            name,
            class,
            argv.to_vec(),
            started.elapsed(),
            log_path.to_path_buf(),
            "could not duplicate the log handle for stderr",
        );
    };

    // argv, never a shell string.
    let mut command = Command::new(program);
    command
        .args(&argv[1..])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    let outcome = match command.status() {
        Ok(status) => match status.code() {
            Some(code) => SpawnOutcome::Exited(code),
            None => SpawnOutcome::Signalled,
        },
        Err(error) => SpawnOutcome::FailedToStart(error.to_string()),
    };

    let _ = log_file.flush();
    let digest = sha256_file(log_path);
    let duration = started.elapsed();

    match outcome {
        SpawnOutcome::Exited(0) => CheckResult::passed(
            name,
            class,
            argv.to_vec(),
            duration,
            log_path.to_path_buf(),
            digest,
        ),
        SpawnOutcome::Exited(code) => CheckResult::subject_failure(
            name,
            class,
            argv.to_vec(),
            Failure {
                exit_status: Some(code),
                duration,
                log_path: log_path.to_path_buf(),
                log_sha256: digest,
                excerpt: Some(excerpt(log_path, 20)),
            },
        ),
        SpawnOutcome::Signalled => CheckResult::harness_error(
            name,
            class,
            argv.to_vec(),
            duration,
            log_path.to_path_buf(),
            format!(
                "{} was terminated by a signal; this is not a verdict about the subject",
                program
            ),
        ),
        SpawnOutcome::FailedToStart(error) => CheckResult {
            name: name.to_owned(),
            class,
            argv: argv.to_vec(),
            exit_status: None,
            // A program that cannot be started is an environment fact, not a
            // statement about the code. Reporting it as a product failure
            // would send someone to debug working code.
            disposition: Disposition::EnvironmentError,
            duration_ms: duration.as_millis() as u64,
            log_path: log_path.to_path_buf(),
            log_sha256: None,
            diagnostic_excerpt: Some(format!("could not start {program}: {error}")),
        },
    }
}

fn try_clone(file: &File) -> std::io::Result<File> {
    file.try_clone()
}

/// A bounded tail of a log, for the console.
///
/// The point of the bound is that a failing gate should be diagnosable from
/// the summary without scrolling past a megabyte of compiler output. The full
/// log stays on disk, referenced by path and digest.
pub fn excerpt(path: &Path, lines: usize) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::from("<log unreadable>");
    };
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

/// SHA-256 of a file's bytes.
///
/// The implementation is written out rather than pulled in as a dependency.
/// This crate exists to be the thing everything else trusts, and adding a
/// hashing dependency to it would move the trust question rather than answer
/// it. It is a few dozen lines, it is deterministic, and `sha256_file` has its
/// own self-test against a known digest.
pub fn sha256_file(path: &Path) -> String {
    let Ok(bytes) = fs::read(path) else {
        return String::from("<unreadable>");
    };
    sha256(&bytes)
}

pub fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finish();
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// A minimal SHA-256.
///
/// Present so that artifact and log digests do not depend on a third-party
/// crate's version or behaviour. Correctness is pinned by `sha256_matches_known_vector`.
pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffered: usize,
    length_bits: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0u8; 64],
            buffered: 0,
            length_bits: 0,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.length_bits = self.length_bits.wrapping_add((data.len() as u64) * 8);
        let mut offset = 0;
        if self.buffered > 0 {
            let take = (64 - self.buffered).min(data.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&data[..take]);
            self.buffered += take;
            offset = take;
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }
        while offset + 64 <= data.len() {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[offset..offset + 64]);
            self.compress(&block);
            offset += 64;
        }
        if offset < data.len() {
            let rest = data.len() - offset;
            self.buffer[..rest].copy_from_slice(&data[offset..]);
            self.buffered = rest;
        }
    }

    pub fn finish(mut self) -> [u8; 32] {
        let bits = self.length_bits;
        self.update_raw(&[0x80]);
        while self.buffered != 56 {
            self.update_raw(&[0x00]);
        }
        self.update_raw(&bits.to_be_bytes());
        let mut out = [0u8; 32];
        for (index, word) in self.state.iter().enumerate() {
            out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    /// Feed padding bytes without disturbing the length counter.
    fn update_raw(&mut self, data: &[u8]) {
        let saved = self.length_bits;
        self.update(data);
        self.length_bits = saved;
    }

    fn compress(&mut self, block: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
}

/// Read the first line of a command's output, or `None`.
///
/// Used only for facts the tool genuinely cannot otherwise observe, such as
/// `rustc -vV`. Never used to decide whether a check passed.
pub fn capture_line(argv: &[String], cwd: &Path) -> Option<String> {
    let rest: &[String] = match argv.get(1..) {
        Some(rest) => rest,
        None => &[],
    };
    let output = Command::new(argv.first()?)
        .args(rest)
        .current_dir(cwd)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .next()
        .map(str::to_owned)
}

/// A stable, human-facing description of a command for reports.
pub fn describe(argv: &[String]) -> String {
    argv.join(" ")
}

/// Where working logs go by default.
///
/// `target/xtask`, never tracked `evidence/`. A qualification run must not
/// dirty the tree it is qualifying, and the earlier Phase 4 scripts had to be
/// repaired specifically because they did.
pub fn default_log_root(root: &Path) -> PathBuf {
    root.join("target").join("xtask")
}

/// The installed R13 activation authority, and the digest it must have.
///
/// This is a host/live fact, deliberately kept out of the CI profile. A CI
/// runner is not the Gnosix workstation: it would fail this check because
/// the file is absent, which is a statement about the runner and not about
/// the code. The repository-hermetic half of the same boundary - the
/// activation authority must contain no process spawn - is a source invariant
/// and does run in CI.
pub const INSTALLED_AUTHORITY_PATH: &str = "/usr/libexec/gnosix/in-gnu-os-activationd";
pub fn expected_authority_sha256() -> &'static str {
    include_str!("../../tools/r13-authority.sha256").trim()
}

/// Evaluate the installed-authority check without spawning a process.
///
/// There is no command here, so there is no exit status, and inventing one
/// would be the same class of error as deriving a verdict from log text. The
/// disposition comes from the digest comparison itself.
pub fn check_installed_authority(
    _root: &Path,
    name: &str,
    class: CheckClass,
    log_path: &Path,
) -> CheckResult {
    check_authority_digest(
        Path::new(INSTALLED_AUTHORITY_PATH),
        expected_authority_sha256(),
        name,
        class,
        log_path,
    )
}

/// Evaluate an authority artifact against an explicit expected digest.
///
/// Keeping the path and digest as inputs lets hermetic tests exercise the real
/// comparison without depending on the machine's installed Gnosix authority.
pub fn check_authority_digest(
    path: &Path,
    expected: &str,
    name: &str,
    class: CheckClass,
    log_path: &Path,
) -> CheckResult {
    let started = std::time::Instant::now();
    let path_display = path.display().to_string();

    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let actual_bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = fs::write(
                log_path,
                format!("checking {path_display} against {expected}\n{path_display}: {error}\n"),
            );
            return CheckResult {
                name: name.to_owned(),
                class,
                argv: vec!["<in-process:authority-digest>".to_owned()],
                exit_status: None,
                // An unreadable/missing artifact is an evaluation environment
                // problem, not evidence that its digest mismatches.
                disposition: Disposition::EnvironmentError,
                duration_ms: started.elapsed().as_millis() as u64,
                log_path: log_path.to_path_buf(),
                log_sha256: Some(sha256_file(log_path)),
                diagnostic_excerpt: Some(format!(
                    "cannot read authority artifact {path_display}: {error}"
                )),
            };
        }
    };

    let actual = sha256(&actual_bytes);
    let _ = fs::write(
        log_path,
        format!("checking {path_display}\nactual   {actual}\nexpected {expected}\n"),
    );
    let digest = sha256_file(log_path);
    let duration = started.elapsed();

    if actual == expected {
        CheckResult {
            name: name.to_owned(),
            class,
            argv: vec!["<in-process:authority-digest>".to_owned()],
            exit_status: None,
            disposition: Disposition::Pass,
            duration_ms: duration.as_millis() as u64,
            log_path: log_path.to_path_buf(),
            log_sha256: Some(digest),
            diagnostic_excerpt: None,
        }
    } else {
        CheckResult::subject_failure(
            name,
            class,
            vec!["<in-process:authority-digest>".to_owned()],
            Failure {
                // No child ran, so there is no exit status. The disposition
                // comes from the digest comparison itself.
                exit_status: None,
                duration,
                log_path: log_path.to_path_buf(),
                log_sha256: digest,
                excerpt: Some(format!(
                    "authority artifact digest is {actual}, expected {expected}"
                )),
            },
        )
    }
}

/// Evaluate one enduring source invariant, without spawning a process.
///
/// The disposition comes from the predicate's own verdict on the file content.
/// There is no command, so there is no exit status, and inventing one would be
/// the same class of error as deriving a verdict from log text.
pub fn check_source_rule(root: &Path, rule: &str, log_path: &Path) -> CheckResult {
    let started = std::time::Instant::now();
    let marker = format!("<in-process:source-rule:{rule}>");

    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let evaluated = crate::sourcegates::evaluate(rule, root);
    let _ = fs::write(log_path, format!("source rule: {rule}\n"));

    match evaluated {
        // The rule could not be evaluated at all - a file it depends on is
        // missing or unreadable. Saying "pass" would be a guess, and saying
        // "fail" would blame code that was never examined.
        Err(error) => {
            let _ = fs::write(log_path, format!("HARNESS_ERROR: {error}\n"));
            CheckResult::harness_error(
                rule,
                CheckClass::RepositoryHermetic,
                vec![marker],
                started.elapsed(),
                log_path.to_path_buf(),
                error,
            )
        }
        Ok(finding) => {
            let _ = fs::write(log_path, format!("{}\n", finding.detail));
            let digest = sha256_file(log_path);
            let duration = started.elapsed();
            if finding.satisfied {
                CheckResult::passed(
                    rule,
                    CheckClass::RepositoryHermetic,
                    vec![marker],
                    duration,
                    log_path.to_path_buf(),
                    digest,
                )
            } else {
                CheckResult::subject_failure(
                    rule,
                    CheckClass::RepositoryHermetic,
                    vec![marker],
                    Failure {
                        exit_status: None,
                        duration,
                        log_path: log_path.to_path_buf(),
                        log_sha256: digest,
                        excerpt: Some(finding.detail),
                    },
                )
            }
        }
    }
}
