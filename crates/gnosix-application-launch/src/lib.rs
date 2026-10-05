//! Typed installed-application launch.
//!
//! A product view decides *which installed application a user picked*. It does
//! not decide what program runs. Everything between those two facts lives here.
//!
//! The shape of this crate exists because of one rule: **GNOS.UX sends an
//! opaque catalog reference, never an `Exec` string and never a shell
//! command.** A launcher that accepts a command line is a remote shell with
//! extra steps, and a launcher that trusts a caller-supplied command line is
//! a remote shell with a confused deputy.
//!
//! So a launch request names a desktop-entry identity and the catalog
//! generation that presented it. Resolution happens here, against a catalog
//! the product cannot mutate, and produces either a fully concrete argv or a
//! typed refusal. There is no partial success and no fallback: an entry this
//! crate cannot honour is refused with a reason rather than launched
//! approximately.
//!
//! The freedesktop launch semantics that matter here are the ones that can
//! quietly do the wrong thing:
//!
//! - **No shell, ever.** An `Exec` template is a token list, not a command
//!   line. It becomes an argv, and it is executed as one. `sh -c` is not
//!   reachable from this API and is not a supported configuration.
//! - **No invented arguments.** An entry whose `Exec` carries `%f` or `%U` is
//!   asking for a document. A launcher has no document, so the field code is
//!   removed rather than filled with an invented path. Removing a required
//!   argument is observable, which is why [`LaunchPlan::substituted_field_codes`]
//!   reports it.
//! - **Freshness is checked, not assumed.** A reference is bound to the
//!   catalog generation that presented it. If the catalog has moved, the
//!   reference is stale and the launch is refused rather than resolved against
//!   an entry the user did not select.
//! - **`TryExec` gates.** Per the specification an entry with a `TryExec` that
//!   does not resolve is not launchable, even though its `Exec` is well formed.
//! - **No terminal guessing.** An entry that needs a terminal is refused unless
//!   a terminal command has been configured by the operator. Picking one on
//!   the user's behalf would be inventing behaviour the product does not own.
//! - **No D-Bus activation fallback.** A `DBusActivatable=true` entry with no
//!   `Exec` needs an activation bus. There is none here, so it is refused
//!   rather than silently run through some other path.
//!
//! Resolution produces a private-field [`LaunchPlan`]. Execution consumes only
//! that validated plan. Production execution belongs to a transient user service,
//! independent of the scene cgroup; tests use an explicit fake executor.

use std::{
    collections::BTreeSet,
    ffi::OsString,
    path::{Path, PathBuf},
};

use gnosix_application_catalog::{
    ApplicationCatalog, CatalogGeneration, DesktopEntryId, ExecTemplate, InstalledApplication,
};

/// One validated launch request: which entry, and the catalog state that
/// presented it.
///
/// This is the only thing a product may send. It is deliberately not
/// constructible from an `Exec` string, a path, or a program name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchReference {
    entry: DesktopEntryId,
    generation: CatalogGeneration,
}

impl LaunchReference {
    /// Bind a catalog entry identity to the generation that presented it.
    pub fn new(entry: DesktopEntryId, generation: CatalogGeneration) -> Self {
        Self { entry, generation }
    }

    pub fn entry(&self) -> &DesktopEntryId {
        &self.entry
    }

    /// The catalog generation this reference is bound to.
    ///
    /// A reference is only meaningful against this generation. It is the
    /// launch equivalent of the semantic generation fence used for workspace
    /// activation.
    pub fn generation(&self) -> &CatalogGeneration {
        &self.generation
    }
}

/// Documents or URLs a user selected, if any.
///
/// A product that launches an application from a launcher passes
/// [`LaunchDocuments::None`]. The variants exist so that a future
/// document-handling surface can be typed rather than smuggled through an
/// `Exec` string.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LaunchDocuments {
    /// The user picked an application, not a document.
    #[default]
    None,
    Files(Vec<PathBuf>),
    Urls(Vec<String>),
}

/// Why a launch could not be produced.
///
/// Every variant is a decision, not an accident. There is deliberately no
/// `Other` or `Unknown` catch-all: an unclassified refusal is a bug, and
/// `Unclassified` exists so that it is visible as one.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LaunchRefusal {
    /// The referenced generation is not the catalog's current generation, so
    /// the entry the user selected is no longer the entry that would run.
    CatalogGenerationChanged { expected: String, current: String },
    /// The catalog no longer contains the referenced entry.
    UnknownEntry,
    /// The entry resolved but is not presentable, and is therefore not
    /// launchable through a product surface.
    NotPresentable,
    /// The entry carries no `Exec` and declares `DBusActivatable=true`, so it
    /// requires D-Bus activation. This crate performs no D-Bus activation.
    RequiresDBusActivation,
    /// The entry carries neither `Exec` nor `DBusActivatable`, so there is
    /// nothing to run.
    NoLaunchMethod,
    /// The entry requires a terminal and no terminal command is configured.
    TerminalRequired,
    /// The entry's `TryExec` did not resolve to an existing executable.
    TryExecMissing { tried: String },
    /// An `Exec` field code has no value available for this launch and the
    /// resulting argv would be wrong rather than merely incomplete.
    UnsupportedFieldCode(char),
    /// Refused after resolution, for a reason specific to the launcher.
    Unclassified(String),
}

impl LaunchRefusal {
    /// Stable machine-readable code, for receipts and logs.
    ///
    /// These strings are evidence surface. Renaming one invalidates
    /// comparisons against retained receipts, so they are part of the
    /// contract rather than debug output.
    pub fn code(&self) -> &'static str {
        match self {
            Self::CatalogGenerationChanged { .. } => "CATALOG_GENERATION_CHANGED",
            Self::UnknownEntry => "UNKNOWN_ENTRY",
            Self::NotPresentable => "NOT_PRESENTABLE",
            Self::RequiresDBusActivation => "REQUIRES_DBUS_ACTIVATION",
            Self::NoLaunchMethod => "NO_LAUNCH_METHOD",
            Self::TerminalRequired => "TERMINAL_REQUIRED",
            Self::TryExecMissing { .. } => "TRY_EXEC_MISSING",
            Self::UnsupportedFieldCode(_) => "UNSUPPORTED_FIELD_CODE",
            Self::Unclassified(_) => "UNCLASSIFIED",
        }
    }
}

/// A launch that has been fully resolved and may be executed.
///
/// Construction is restricted to this module: the only way to obtain one is
/// for a [`LaunchPlan`] to succeed. A caller cannot assemble an argv by hand
/// and call it a plan, which is what keeps "no shell" and "no invented
/// arguments" from being conventions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchPlan {
    entry: DesktopEntryId,
    generation: CatalogGeneration,
    program: OsString,
    args: Vec<OsString>,
    /// Field codes that were present in the `Exec` and were removed because
    /// this launch had no value for them.
    substituted_field_codes: BTreeSet<char>,
    /// True when the argv was wrapped in a configured terminal command.
    via_terminal: bool,
}

impl LaunchPlan {
    /// The desktop entry this plan will run.
    pub fn entry(&self) -> &DesktopEntryId {
        &self.entry
    }

    /// The catalog generation the entry was resolved against.
    pub fn generation(&self) -> &CatalogGeneration {
        &self.generation
    }

    /// The program that will be executed. Never a shell.
    pub fn program(&self) -> &OsStr {
        self.program.as_os_str()
    }

    pub fn args(&self) -> &[OsString] {
        &self.args
    }

    /// Field codes dropped because this launch supplied no value for them.
    ///
    /// A non-empty set is a visible deviation from the entry's declared
    /// arguments and is carried in the receipt.
    pub fn substituted_field_codes(&self) -> &BTreeSet<char> {
        &self.substituted_field_codes
    }

    /// Whether the plan runs inside a configured terminal.
    pub fn via_terminal(&self) -> bool {
        self.via_terminal
    }

    /// A copy of the plan as the argument vector it will execute.
    ///
    /// Exposed for logging and qualification. It is the same vector
    /// an executor receives, so a recorded plan is a true record.
    pub fn argv(&self) -> Vec<&OsStr> {
        std::iter::once(self.program.as_os_str())
            .chain(self.args.iter().map(OsString::as_os_str))
            .collect()
    }
}

use std::ffi::OsStr;

#[cfg(feature = "controlled-direct")]
mod direct;
mod execution;
#[cfg(feature = "controlled-direct")]
pub use direct::DirectLaunchExecutor;
pub use execution::{
    LaunchExecutionError, LaunchExecutionOwner, LaunchExecutionReceipt, LaunchExecutor,
    SystemdUserLaunchExecutor,
};

/// The outcome of a launch attempt, in the shape a product records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchOutcome {
    /// The process was started.
    Launched {
        pid: u32,
        owner: LaunchExecutionOwner,
        program: OsString,
        dropped_field_codes: BTreeSet<char>,
        via_terminal: bool,
    },
    /// The entry was resolved but the process could not be started.
    SpawnFailed {
        program: OsString,
        error: LaunchExecutionError,
    },
    /// The launch was refused before anything was started.
    Refused(LaunchRefusal),
}

impl LaunchOutcome {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Launched { .. } => "LAUNCHED",
            Self::SpawnFailed { .. } => "SPAWN_FAILED",
            Self::Refused(_) => "REFUSED",
        }
    }

    /// Whether this outcome represents a started application.
    pub fn is_launched(&self) -> bool {
        matches!(self, Self::Launched { .. })
    }
}

/// A deterministic receipt for one launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchReceipt {
    pub entry: DesktopEntryId,
    pub generation: CatalogGeneration,
    pub outcome: LaunchOutcome,
}

impl LaunchReceipt {
    /// A single-line rendering suitable for a product log line.
    ///
    /// The receipt is the evidence, so it states the entry, the generation it
    /// was resolved against, and the outcome code. It never claims a launch
    /// succeeded unless `is_launched` is true, and it never renders an
    /// arbitrary program name as if the product had chosen it.
    pub fn summary(&self) -> String {
        let execution = match &self.outcome {
            LaunchOutcome::Launched { pid, owner, .. } => format!(" pid={pid} owner={owner}"),
            LaunchOutcome::SpawnFailed { error, .. } => format!(" execution_error={error}"),
            LaunchOutcome::Refused(_) => String::new(),
        };
        format!(
            "entry={} generation={} outcome={}{}",
            self.entry.as_str(),
            self.generation.hex(),
            self.outcome.code(),
            execution
        )
    }
}

/// Configuration for the trusted launch capability.
///
/// The terminal command is operator configuration, not a product choice.
/// Leaving it unset means terminal applications are refused, which is the
/// honest behaviour when this machine has no configured terminal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchConfiguration {
    /// Command used to run terminal applications, as an argv template.
    ///
    /// The first element is the terminal; the rest are prefixed to the
    /// application's argv. This is never chosen automatically.
    terminal_command: Vec<OsString>,
}

impl LaunchConfiguration {
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure the terminal used for `Terminal=true` entries.
    pub fn with_terminal_command(mut self, command: impl IntoIterator<Item = OsString>) -> Self {
        self.terminal_command = command.into_iter().collect();
        self
    }

    pub fn terminal_command(&self) -> &[OsString] {
        &self.terminal_command
    }
}

/// The bounded installed-application launch capability.
///
/// Construction is cheap and holds no process, socket or session state. Every
/// decision this type makes is a pure function of the request, the catalog
/// and the configuration, which is why the interesting behaviour is testable
/// without launching anything.
#[derive(Clone, Debug, Default)]
pub struct ApplicationLauncher {
    configuration: LaunchConfiguration,
}

impl ApplicationLauncher {
    pub fn new(configuration: LaunchConfiguration) -> Self {
        Self { configuration }
    }

    pub fn configuration(&self) -> &LaunchConfiguration {
        &self.configuration
    }

    /// Resolve a request into an executable plan, or refuse.
    ///
    /// Resolution is performed against `catalog` as it exists now. The
    /// request's recorded generation is checked first: if the catalog has
    /// moved on, the entry the user picked may no longer be the entry that
    /// would run, and this refuses rather than guessing which is correct.
    pub fn plan(
        &self,
        request: &LaunchReference,
        catalog: &ApplicationCatalog,
        documents: &LaunchDocuments,
    ) -> Result<LaunchPlan, LaunchRefusal> {
        let current = catalog.generation().clone();
        if current != *request.generation() {
            return Err(LaunchRefusal::CatalogGenerationChanged {
                expected: request.generation().hex(),
                current: current.hex(),
            });
        }

        let entry = catalog
            .get(request.entry())
            .ok_or(LaunchRefusal::UnknownEntry)?;

        if !entry.normally_visible() || !entry.visible_in(catalog.desktop_environment()) {
            return Err(LaunchRefusal::NotPresentable);
        }

        if let Some(try_exec) = entry.try_exec.as_deref()
            && !try_exec_present(try_exec)
        {
            return Err(LaunchRefusal::TryExecMissing {
                tried: try_exec.to_owned(),
            });
        }

        let Some(template) = entry.exec.as_ref() else {
            return Err(if entry.dbus_activatable {
                LaunchRefusal::RequiresDBusActivation
            } else {
                LaunchRefusal::NoLaunchMethod
            });
        };

        if entry.terminal && self.configuration.terminal_command().is_empty() {
            return Err(LaunchRefusal::TerminalRequired);
        }

        let substitution = FieldSubstitution::for_documents(documents);
        let (program, args, dropped) = build_argv(template, &substitution, entry)?;

        if entry.terminal {
            let wrapped = self.wrap_in_terminal(program, args);
            return Ok(LaunchPlan {
                entry: entry.id.clone(),
                generation: current,
                program: wrapped.0,
                args: wrapped.1,
                substituted_field_codes: dropped,
                via_terminal: true,
            });
        }

        Ok(LaunchPlan {
            entry: entry.id.clone(),
            generation: current,
            program,
            args,
            substituted_field_codes: dropped,
            via_terminal: false,
        })
    }

    /// Resolve and start, returning a receipt either way.
    ///
    /// This is the whole capability in one call for callers that do not need
    /// to inspect the plan. The user manager owns the application lifetime;
    /// stopping the scene does not stop its independent transient service.
    pub fn launch(
        &self,
        request: &LaunchReference,
        catalog: &ApplicationCatalog,
        documents: &LaunchDocuments,
    ) -> LaunchReceipt {
        self.launch_with_executor(request, catalog, documents, &SystemdUserLaunchExecutor)
    }

    /// Execute a validated plan through an explicitly selected backend.
    /// The executor never receives an unresolved product request.
    pub fn launch_with_executor(
        &self,
        request: &LaunchReference,
        catalog: &ApplicationCatalog,
        documents: &LaunchDocuments,
        executor: &dyn LaunchExecutor,
    ) -> LaunchReceipt {
        match self.plan(request, catalog, documents) {
            Ok(plan) => match executor.execute(&plan) {
                Ok(process) => LaunchReceipt {
                    entry: plan.entry().clone(),
                    generation: plan.generation().clone(),
                    outcome: LaunchOutcome::Launched {
                        pid: process.pid,
                        owner: process.owner,
                        program: plan.program().to_owned(),
                        dropped_field_codes: plan.substituted_field_codes().clone(),
                        via_terminal: plan.via_terminal(),
                    },
                },
                Err(error) => LaunchReceipt {
                    entry: plan.entry().clone(),
                    generation: plan.generation().clone(),
                    outcome: LaunchOutcome::SpawnFailed {
                        program: plan.program().to_owned(),
                        error,
                    },
                },
            },
            Err(refusal) => LaunchReceipt {
                entry: request.entry().clone(),
                generation: request.generation().clone(),
                outcome: LaunchOutcome::Refused(refusal),
            },
        }
    }

    fn wrap_in_terminal(
        &self,
        program: OsString,
        args: Vec<OsString>,
    ) -> (OsString, Vec<OsString>) {
        let terminal = self.configuration().terminal_command().to_vec();
        let mut all = Vec::with_capacity(terminal.len() + 1 + args.len());
        all.extend_from_slice(&terminal);
        all.push(program);
        all.extend_from_slice(&args);
        let program = all.remove(0);
        (program, all)
    }
}

/// Values available for `Exec` field codes in one launch.
struct FieldSubstitution {
    files: Vec<String>,
    urls: Vec<String>,
}

impl FieldSubstitution {
    fn for_documents(documents: &LaunchDocuments) -> Self {
        match documents {
            LaunchDocuments::None => Self {
                files: Vec::new(),
                urls: Vec::new(),
            },
            LaunchDocuments::Files(files) => Self {
                files: files
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect(),
                urls: Vec::new(),
            },
            LaunchDocuments::Urls(urls) => Self {
                files: Vec::new(),
                urls: urls.clone(),
            },
        }
    }

    fn has_files(&self) -> bool {
        !self.files.is_empty()
    }

    fn has_urls(&self) -> bool {
        !self.urls.is_empty()
    }
}

/// Build a concrete argv from a template, reporting dropped field codes.
///
/// The subtle part is deciding when a field-code token still contributes an
/// argument. A token can be:
///
/// - pure literal text - always contributes it;
/// - a single field code that expanded to text - contributes it;
/// - a multi-value field code that expanded to several arguments - contributes
///   those arguments and *not* an empty trailing one;
/// - a field code with no value available - contributes nothing at all, even
///   if the token had literal text around the code, because a flag whose
///   required value is absent is not a valid argument and `--file=` is worse
///   than no flag.
///
/// Getting the third case wrong produces a stray empty argument, which is
/// exactly the kind of defect that only shows up as mysterious behaviour in a
/// launched program.
fn build_argv(
    template: &ExecTemplate,
    substitution: &FieldSubstitution,
    entry: &InstalledApplication,
) -> Result<(OsString, Vec<OsString>, BTreeSet<char>), LaunchRefusal> {
    let mut argv: Vec<OsString> = Vec::with_capacity(template.argv.len());
    let mut dropped = BTreeSet::new();

    for token in &template.argv {
        let mut current = String::with_capacity(token.len());
        let mut chars = token.chars().peekable();
        // Literal text in this token, from any source.
        let mut has_content = false;
        // This token already emitted arguments as separate entries.
        let mut pushed_values = false;
        // A field code in this token had no value.
        let mut lost_value = false;

        while let Some(ch) = chars.next() {
            if ch != '%' {
                if !ch.is_whitespace() {
                    has_content = true;
                }
                current.push(ch);
                continue;
            }
            let Some(code) = chars.next() else {
                return Err(LaunchRefusal::UnsupportedFieldCode('%'));
            };
            match code {
                '%' => {
                    current.push('%');
                    has_content = true;
                }
                'i' => {
                    // The icon form is defined as its own two arguments; the
                    // inlined form is what implementations commonly do when the
                    // code appears inside a larger token.
                    current.push_str(entry.icon.as_deref().unwrap_or_default());
                    has_content = true;
                }
                'c' => {
                    current.push_str(&entry.name);
                    has_content = true;
                }
                'k' => {
                    current.push_str(&entry.source_path.display().to_string());
                    has_content = true;
                }
                'f' | 'F' | 'u' | 'U' => {
                    let values: Option<Vec<&str>> = match code {
                        'f' | 'F' => substitution
                            .has_files()
                            .then(|| substitution.files.iter().map(String::as_str).collect()),
                        _ => substitution
                            .has_urls()
                            .then(|| substitution.urls.iter().map(String::as_str).collect()),
                    };
                    match values {
                        Some(values) if code == 'F' || code == 'U' => {
                            for value in values {
                                if has_content {
                                    argv.push(OsString::from(std::mem::take(&mut current)));
                                    has_content = false;
                                }
                                argv.push(OsString::from(value));
                                pushed_values = true;
                            }
                        }
                        Some(values) => {
                            current.push_str(values[0]);
                            has_content = true;
                        }
                        None => {
                            dropped.insert(code);
                            lost_value = true;
                        }
                    }
                }
                other => return Err(LaunchRefusal::UnsupportedFieldCode(other)),
            }
        }

        // A token whose field code had no value contributes nothing, and a
        // token fully consumed by a multi-value code does not contribute an
        // empty trailing argument.
        if !lost_value && (has_content || !pushed_values) {
            argv.push(OsString::from(current));
        }
    }

    let Some(program) = argv.first().cloned() else {
        return Err(LaunchRefusal::NoLaunchMethod);
    };
    let args = argv.split_off(1);
    Ok((program, args, dropped))
}

/// Resolve a `TryExec` value the way the specification requires.
///
/// An absolute path is used as-is. A bare name is looked up on `PATH`. A
/// value with a path separator is treated as a relative or absolute path and
/// is checked directly, which is what stops a bare name from resolving to
/// something in the current directory.
fn try_exec_present(try_exec: &str) -> bool {
    let candidate = Path::new(try_exec);
    if candidate.components().count() > 1 || candidate.is_absolute() {
        return is_executable_file(candidate);
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| is_executable_file(&dir.join(try_exec)))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests;
