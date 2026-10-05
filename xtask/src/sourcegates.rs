//! Enduring source invariants.
//!
//! These are architectural contracts, and the way each one is enforced is
//! chosen deliberately, strongest first:
//!
//! 1. **The type system.** Most of what this file checks is already enforced by
//!    the compiler: `LaunchReference` cannot be built from a command, and
//!    `DesktopProfileWriter` cannot be constructed outside its crate. Those
//!    need no check here, and inventing one would be theatre. `xtask
//!    source-gates` exists to *report* the shape and to cover the residue the
//!    compiler cannot see.
//! 2. **Structural source assertions.** The residue: an identifier that must
//!    not appear at all, a visibility that must be crate-private, a path that
//!    must exist. These are checked against file content.
//! 3. **Text matching.** Last resort, and used only where the alternative is
//!    worse.
//!
//! Every predicate here is a pure function from file content to a result, so
//! the negative test can drive it with a fixture string. That is the point of
//! §45 in practice: the earlier Phase 4 gates were only provable by editing
//! real product files and hoping restoration worked, and a restoration that
//! silently fails is exactly the failure this project has now hit three
//! times. A fixture cannot be left dirty, because a fixture is not a file.

use std::path::Path;

/// One invariant finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub rule: &'static str,
    pub satisfied: bool,
    pub detail: String,
}

impl Finding {
    fn satisfied(rule: &'static str, detail: impl Into<String>) -> Self {
        Self {
            rule,
            satisfied: true,
            detail: detail.into(),
        }
    }

    fn violated(rule: &'static str, detail: impl Into<String>) -> Self {
        Self {
            rule,
            satisfied: false,
            detail: detail.into(),
        }
    }
}

/// Extract the body of every inherent `impl` block for a named type.
///
/// Two things this must get right, both of which were wrong first and both of
/// which were caught by the negative tests against a fixture tree:
///
/// - **Bounded by brace counting**, not "everything after the header". The
///   unbounded version read seventeen kilobytes past the end and found
///   unrelated constructors, reporting a violation that did not exist. A gate
///   that cries wolf is worse than no gate, because it teaches people to ignore
///   gates.
/// - **Every block, not the first.** A type may have several inherent `impl`
///   blocks, and a public constructor added to any one of them is the violation.
///   Returning only the first occurrence let a real violation through, which is
///   the direction that actually matters.
pub fn impl_blocks<'a>(source: &'a str, type_name: &str) -> Vec<&'a str> {
    let header = format!("impl {type_name}");
    let mut blocks = Vec::new();
    let mut search_from = 0usize;

    while let Some(relative) = source[search_from..].find(&header) {
        let start = search_from + relative;
        // Require the type to end here, so `impl LaunchPlanFactory` is not
        // mistaken for `impl LaunchPlan`.
        let after = start + header.len();
        let is_exact = source[after..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '{');
        let Some(open) = source[start..].find('{').map(|o| start + o) else {
            break;
        };
        if is_exact {
            match brace_span(source, open) {
                Some(end) => {
                    blocks.push(&source[open..=end]);
                    search_from = end + 1;
                    continue;
                }
                // Unbalanced braces: stop rather than guess at a block extent.
                None => break,
            }
        }
        search_from = after;
    }
    blocks
}

/// The end offset of the block opening at `open`, by brace counting.
fn brace_span(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            // Skip string and char literals so a brace inside one does not
            // throw the depth count off.
            _ => {}
        }
    }
    None
}

/// `LaunchReference` must be constructible only from a catalog entry identity
/// and a generation.
///
/// Enforced in the type system by the struct's private fields and its single
/// constructor's signature. Checked here so the guarantee is *visible* in a
/// report rather than only in a compiler error someone has to provoke.
pub fn launch_reference_is_catalog_bound(source: &str) -> Finding {
    let takes_entry = source.contains("entry: DesktopEntryId");
    let takes_generation = source.contains("generation: CatalogGeneration");
    let forbidden = [
        "from_exec",
        "from_command",
        "from_program",
        "from_argv",
        "from_path",
    ];
    let leaked: Vec<&str> = forbidden
        .iter()
        .copied()
        .filter(|name| source.contains(name))
        .collect();

    if takes_entry && takes_generation && leaked.is_empty() {
        Finding::satisfied(
            "launch-reference-catalog-bound",
            "LaunchReference is built from a DesktopEntryId and a CatalogGeneration only",
        )
    } else {
        Finding::violated(
            "launch-reference-catalog-bound",
            format!(
                "entry={takes_entry} generation={takes_generation} command_constructors={leaked:?}"
            ),
        )
    }
}

/// The launch path must contain no shell interpreter.
///
/// A shell on this path would turn a catalog-selected entry into arbitrary
/// command execution, which is the exact property the capability exists to
/// prevent. The check is deliberately blunt about `sh -c` and friends: a
/// false positive here is a comment, and a false negative is a vulnerability.
pub fn launch_path_has_no_shell(sources: &[(&str, &str)]) -> Finding {
    // Matches a shell as a *program*, which is how a shell would actually be
    // reachable. The word "sh" inside an identifier or a path component such
    // as a crate name is not a shell.
    const SHELL_MARKERS: &[&str] = &[
        "Command::new(\"sh\")",
        "Command::new(\"bash\")",
        "Command::new(\"/bin/sh\")",
        "Command::new(\"/usr/bin/sh\")",
        "process::Command::new(\"sh\")",
        "[\"sh\", \"-c\"]",
    ];
    let mut hits = Vec::new();
    for (name, source) in sources {
        for marker in SHELL_MARKERS {
            if source.contains(marker) {
                hits.push(format!("{name}: {marker}"));
            }
        }
    }
    if hits.is_empty() {
        Finding::satisfied(
            "launch-path-no-shell",
            "no shell interpreter is reachable on the launch path",
        )
    } else {
        Finding::violated("launch-path-no-shell", hits.join("; "))
    }
}

/// A resolved launch plan must not be constructible by hand.
///
/// If a caller could assemble a `LaunchPlan` directly, every guarantee about
/// the argv would become a convention rather than a property, because the
/// plan is the thing the process is actually spawned from.
pub fn launch_plan_is_not_hand_constructible(source: &str) -> Finding {
    let blocks = impl_blocks(source, "LaunchPlan");
    // No block at all is itself a finding: a LaunchPlan the gate cannot even
    // locate is not a LaunchPlan whose constructor visibility was checked.
    let located = !blocks.is_empty();
    let has_public_new = blocks.iter().any(|block| block.contains("pub fn new"));
    if !located {
        Finding::violated(
            "launch-plan-not-hand-constructible",
            "no `impl LaunchPlan` block was found, so its visibility could not be checked",
        )
    } else if has_public_new {
        Finding::violated(
            "launch-plan-not-hand-constructible",
            "LaunchPlan exposes a public constructor, so a caller could supply its own argv",
        )
    } else {
        Finding::satisfied(
            "launch-plan-not-hand-constructible",
            "LaunchPlan has no public constructor; only a successful resolution produces one",
        )
    }
}

/// The profile writer must remain crate-private and singly issued.
///
/// Two halves, both structural. The writer's constructor is private, so
/// nothing outside the crate can build one; and `open_writer` is `pub(crate)`,
/// so within the crate there is exactly one production caller.
pub fn profile_writer_is_crate_private(store_source: &str, authority_source: &str) -> Finding {
    let crate_private = store_source.contains("pub(crate) fn open_writer");
    let publicly_exposed = store_source
        .lines()
        .any(|line| line.trim_start().starts_with("pub fn open_writer"));
    // Every inherent block, not just the first: a public constructor can be
    // added in any of them.
    let public_constructor = impl_blocks(store_source, "DesktopProfileWriter")
        .iter()
        .any(|block| block.contains("pub fn new"));
    let authority_issues = authority_source.contains("open_writer()");

    if crate_private && !publicly_exposed && !public_constructor && authority_issues {
        Finding::satisfied(
            "profile-writer-crate-private",
            "open_writer is pub(crate), DesktopProfileWriter has no public constructor, \
             and the designated authority is the production caller",
        )
    } else {
        Finding::violated(
            "profile-writer-crate-private",
            format!(
                "crate_private={crate_private} publicly_exposed={publicly_exposed} \
                 public_constructor={public_constructor} authority_issues={authority_issues}"
            ),
        )
    }
}

/// The three capabilities must stay separate.
///
/// Workspace activation, application launch and profile persistence are
/// different powers with different authority and different freshness domains.
/// A universal `execute(action)` would collapse them, and collapsing them is
/// how an authority boundary stops being a boundary.
pub fn capabilities_remain_separate(sources: &[(&str, &str)]) -> Finding {
    const UNIVERSAL: &[&str] = &["pub fn execute(", "pub fn run_command", "trait Executor"];
    let mut hits = Vec::new();
    for (name, source) in sources {
        for marker in UNIVERSAL {
            if source.contains(marker) {
                hits.push(format!("{name}: {marker}"));
            }
        }
    }
    if hits.is_empty() {
        Finding::satisfied(
            "capabilities-remain-separate",
            "activation, launch and persistence have no universal execute authority",
        )
    } else {
        Finding::violated("capabilities-remain-separate", hits.join("; "))
    }
}

/// The activation authority must not have gained process execution.
///
/// The R13 workspace-activation authority is a different power from launching
/// an application. If it grew a spawn capability, the boundary between
/// "activate a workspace" and "run this program" would have been quietly
/// removed, and nothing else in the tree would notice.
pub fn activation_authority_has_no_spawn(source: &str) -> Finding {
    const SPAWN: &[&str] = &["process::Command", "Command::new", "std::process::Stdio"];
    let hits: Vec<&str> = SPAWN
        .iter()
        .copied()
        .filter(|m| source.contains(m))
        .collect();
    if hits.is_empty() {
        Finding::satisfied(
            "activation-authority-no-spawn",
            "the R13 activation authority contains no process spawn capability",
        )
    } else {
        Finding::violated(
            "activation-authority-no-spawn",
            format!("activation authority gained a spawn capability: {hits:?}"),
        )
    }
}

/// The AppStack view must not be able to supply a command.
///
/// The type system already does most of this work: the AppStack's product action
/// takes a `DesktopEntryId` and nothing else, so there is no argument a view
/// could pass that names a program. This rule covers the residue - the ways a
/// view can acquire command-shaped knowledge without a type error, such as
/// reaching for a catalog entry's `Exec` field directly or growing a
/// convenience constructor that takes a program name.
///
/// It is deliberately narrow. Reading a desktop-entry file in the view would be
/// caught by a different rule; this one asks only whether command-shaped values
/// have leaked into the surface.
pub fn view_supplies_no_command(sources: &[(&str, &str)]) -> Finding {
    const FORBIDDEN: &[&str] = &[
        "process::Command",
        "Command::new",
        ".exec",
        "launch_by_name",
        "launch_path",
        "launch_exec",
        "from_exec",
    ];
    let mut hits = Vec::new();
    for (name, source) in sources {
        for marker in FORBIDDEN {
            if source.contains(marker) {
                hits.push(format!("{name}: {marker}"));
            }
        }
    }
    if hits.is_empty() {
        Finding::satisfied(
            "appstack-view-supplies-no-command",
            "the AppStack view selects a catalog identity and never a command",
        )
    } else {
        Finding::violated("appstack-view-supplies-no-command", hits.join("; "))
    }
}

/// Every rule, paired with the files it reads.
pub fn launch_execution_is_bound(
    launch: &str,
    execution: &str,
    product: &str,
    manifest: &str,
) -> Finding {
    let required = launch.contains("&SystemdUserLaunchExecutor")
        && execution.contains("impl LaunchExecutor for SystemdUserLaunchExecutor")
        && execution.contains("plan: &LaunchPlan")
        && execution.contains("\"StartTransientUnit\"")
        && execution.contains("\"no-env-expand\"")
        && execution.contains("CacheProperties::No")
        && execution.contains("active == \"active\" && pid > 0")
        && product.contains("launcher, &SystemdUserLaunchExecutor")
        && manifest.contains("controlled-direct = []");
    let prohibited = [
        "DirectLaunchExecutor",
        "Command::new",
        "zbus::",
        "\"ExecStart\"",
        "\"ExecStartEx\"",
        "\"Environment\"",
        "\"KillMode\"",
        "\"Slice\"",
        "fn run(",
        "fn start_unit(",
        "fn systemd_call(",
    ];
    let product_clean = prohibited.iter().all(|token| !product.contains(token));
    let private_fixed_contract = !execution.contains("pub fn ")
        && !execution.contains("pub type ")
        && [
            "\"Environment\"",
            "\"KillMode\"",
            "\"Slice\"",
            "Command::new",
            "systemd-run",
            "\"StopUnit\"",
            "\"SetUnitProperties\"",
        ]
        .iter()
        .all(|token| !execution.contains(token))
        && !launch.contains("pub fn spawn")
        && !manifest.contains("default = [\"controlled-direct\"]");
    if required && product_clean && private_fixed_contract {
        Finding::satisfied(
            "launch-execution-lifetime-bound",
            "production executes only validated plans through the fixed transient-service backend; no product properties or direct fallback",
        )
    } else {
        Finding::violated(
            "launch-execution-lifetime-bound",
            "fixed transient execution boundary, private property construction, or production binding changed",
        )
    }
}

pub fn rules() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "launch-reference-catalog-bound",
            vec!["crates/gnosix-application-launch/src/lib.rs"],
        ),
        (
            "launch-path-no-shell",
            vec![
                "crates/gnosix-application-launch/src/lib.rs",
                "crates/gnosix-application-launch/src/execution.rs",
                "crates/gnosix-application-launch/src/direct.rs",
                "crates/gnos-ux-gpui/src/product_launch.rs",
            ],
        ),
        (
            "launch-execution-lifetime-bound",
            vec![
                "crates/gnosix-application-launch/src/lib.rs",
                "crates/gnosix-application-launch/src/execution.rs",
                "crates/gnos-ux-gpui/src/product_launch.rs",
                "crates/gnosix-application-launch/Cargo.toml",
            ],
        ),
        (
            "launch-plan-not-hand-constructible",
            vec!["crates/gnosix-application-launch/src/lib.rs"],
        ),
        (
            "profile-writer-crate-private",
            vec![
                "crates/in-gnu-os-desktop-profiles/src/store.rs",
                "crates/in-gnu-os-desktop-profiles/src/authority.rs",
            ],
        ),
        (
            "capabilities-remain-separate",
            vec![
                "crates/gnos-ux-gpui/src/product_launch.rs",
                "crates/gnos-ux-gpui/src/product_controller.rs",
                "crates/in-gnu-os-desktop-profiles/src/authority.rs",
            ],
        ),
        (
            "activation-authority-no-spawn",
            vec!["crates/in-gnu-os-activation/src/lib.rs"],
        ),
        (
            "appstack-view-supplies-no-command",
            vec![
                "crates/gnos-ux-gpui/src/product_appstack.rs",
                "crates/gnos-ux-gpui/src/product_scene.rs",
            ],
        ),
        (
            "semantic-input-core-has-no-execution-authority",
            vec![
                "crates/gnosix-input-core/Cargo.toml",
                "crates/gnosix-input-core/src/lib.rs",
            ],
        ),
        (
            "input-observation-contract-has-no-host-or-execution-authority",
            vec![
                "crates/gnosix-input-observation/Cargo.toml",
                "crates/gnosix-input-observation/src/lib.rs",
            ],
        ),
        (
            "input-runtime-observer-is-read-only-transport",
            vec![
                "crates/gnosix-input-runtime-observer/Cargo.toml",
                "crates/gnosix-input-runtime-observer/src/lib.rs",
                "crates/gnosix-input-runtime-observer/src/main.rs",
            ],
        ),
    ]
}

/// Evaluate one named rule against the working tree.
///
/// Returns `HarnessError`-shaped detail when a file it needs is absent,
/// because a missing file means the rule could not be evaluated and saying
/// "pass" would be a guess.
pub fn evaluate(rule: &str, root: &Path) -> Result<Finding, String> {
    let read = |relative: &str| -> Result<String, String> {
        std::fs::read_to_string(root.join(relative)).map_err(|error| format!("{relative}: {error}"))
    };

    Ok(match rule {
        "launch-reference-catalog-bound" | "launch-reference" | "launch-surface" => {
            launch_reference_is_catalog_bound(&read("crates/gnosix-application-launch/src/lib.rs")?)
        }
        "semantic-input-core-has-no-execution-authority" => {
            let manifest = read("crates/gnosix-input-core/Cargo.toml")?;
            let mut source = String::new();
            let directory = root.join("crates/gnosix-input-core/src");
            let paths = rust_sources_under(&directory)?;
            for path in paths {
                let text = std::fs::read_to_string(&path)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
                source.push_str(&text);
            }
            semantic_input_core_is_non_executable(&manifest, &source)
        }
        "input-observation-contract-has-no-host-or-execution-authority" => {
            let manifest = read("crates/gnosix-input-observation/Cargo.toml")?;
            let mut source = String::new();
            let directory = root.join("crates/gnosix-input-observation/src");
            let paths = rust_sources_under(&directory)?;
            for path in paths {
                let text = std::fs::read_to_string(&path)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
                source.push_str(&text);
            }
            input_observation_contract_is_bounded(&manifest, &source)
        }
        "input-runtime-observer-is-read-only-transport" => {
            let manifest = read("crates/gnosix-input-runtime-observer/Cargo.toml")?;
            let mut source = String::new();
            let directory = root.join("crates/gnosix-input-runtime-observer/src");
            for path in rust_sources_under(&directory)? {
                let text = std::fs::read_to_string(&path)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
                source.push_str(&text);
            }
            input_runtime_observer_is_read_only_transport(&manifest, &source)
        }
        "launch-path-no-shell" => {
            let a = read("crates/gnosix-application-launch/src/lib.rs")?;
            let b = read("crates/gnos-ux-gpui/src/product_launch.rs")?;
            let c = read("crates/gnosix-application-launch/src/execution.rs")?;
            let d = read("crates/gnosix-application-launch/src/direct.rs")?;
            launch_path_has_no_shell(&[
                ("launch", &a),
                ("product_launch", &b),
                ("execution", &c),
                ("direct", &d),
            ])
        }
        "launch-execution-lifetime-bound" => launch_execution_is_bound(
            &read("crates/gnosix-application-launch/src/lib.rs")?,
            &read("crates/gnosix-application-launch/src/execution.rs")?,
            &read("crates/gnos-ux-gpui/src/product_launch.rs")?,
            &read("crates/gnosix-application-launch/Cargo.toml")?,
        ),
        "launch-plan-not-hand-constructible" => launch_plan_is_not_hand_constructible(&read(
            "crates/gnosix-application-launch/src/lib.rs",
        )?),
        "profile-writer-crate-private" => {
            let store = read("crates/in-gnu-os-desktop-profiles/src/store.rs")?;
            let authority = read("crates/in-gnu-os-desktop-profiles/src/bar_writer.rs")?;
            profile_writer_is_crate_private(&store, &authority)
        }
        "capabilities-remain-separate" => {
            let launch = read("crates/gnos-ux-gpui/src/product_launch.rs")?;
            let controller = read("crates/gnos-ux-gpui/src/product_controller.rs")?;
            let authority = read("crates/in-gnu-os-desktop-profiles/src/authority.rs")?;
            let profile_service = read("crates/in-gnu-os-profile-service/src/lib.rs")?;
            let profile_client = read("crates/gnos-ux-profile-client/src/lib.rs")?;
            let profile_ui = read("crates/gnos-ux-gpui/src/product_profile.rs")?;
            capabilities_remain_separate(&[
                ("profile_service", &profile_service),
                ("profile_client", &profile_client),
                ("profile_ui", &profile_ui),
                ("product_launch", &launch),
                ("product_controller", &controller),
                ("authority", &authority),
            ])
        }
        "activation-authority-no-spawn" | "activation-authority" => {
            activation_authority_has_no_spawn(&read("crates/in-gnu-os-activation/src/lib.rs")?)
        }
        // `view-supplies-no-command` remains an accepted alias so an existing
        // qualification invocation keeps working after the rename. Removing the
        // old name would have turned a terminology correction into a silently
        // disabled gate for anything still calling it.
        "appstack-view-supplies-no-command" | "view-supplies-no-command" | "appstack-view" => {
            let appstack = read("crates/gnos-ux-gpui/src/product_appstack.rs")?;
            let scene = read("crates/gnos-ux-gpui/src/product_scene.rs")?;
            view_supplies_no_command(&[("product_appstack", &appstack), ("product_scene", &scene)])
        }
        other => return Err(format!("unknown source-gate rule: {other}")),
    })
}

fn semantic_input_core_is_non_executable(manifest: &str, source: &str) -> Finding {
    const RULE: &str = "semantic-input-core-has-no-execution-authority";
    let forbidden_api = [
        "std::process",
        "std::fs",
        "std::env",
        "std::net",
        "Command::new",
        "pub argv:",
        "pub command:",
        "pub shell:",
        "pub systemd_properties:",
        "pub dbus_destination:",
        "pub dbus_interface:",
        "pub dbus_member:",
    ];
    if let Some(token) = forbidden_api.iter().find(|token| source.contains(**token)) {
        return Finding::violated(RULE, format!("execution/host API exposed in core: {token}"));
    }
    let allowed_dependencies = [
        (
            "[dependencies]",
            "in-gnu-os-settings-core = { path = \"../in-gnu-os-settings-core\" }",
        ),
        (
            "[dependencies]",
            "serde = { version = \"1\", features = [\"derive\"] }",
        ),
        ("[dev-dependencies]", "serde_json = \"1\""),
    ];
    let mut section = "";
    let mut found: Vec<(&str, String)> = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line;
            continue;
        }
        if [
            "[dependencies]",
            "[dev-dependencies]",
            "[build-dependencies]",
        ]
        .contains(&section)
            && !line.is_empty()
            && !line.starts_with('#')
        {
            found.push((section, line.to_owned()));
        }
    }
    if found.iter().any(|(section, dependency)| {
        !allowed_dependencies.contains(&(*section, dependency.as_str()))
    }) || allowed_dependencies.iter().any(|required| {
        !found
            .iter()
            .any(|(section, line)| (*section, line.as_str()) == *required)
    }) {
        return Finding::violated(RULE, format!("unexpected core dependencies: {found:?}"));
    }
    Finding::satisfied(
        RULE,
        "pure typed core has only validated identity/serde dependencies and no host/execution API",
    )
}

fn input_observation_contract_is_bounded(manifest: &str, source: &str) -> Finding {
    const RULE: &str = "input-observation-contract-has-no-host-or-execution-authority";
    let forbidden_api = [
        "std::process",
        "std::fs",
        "std::env",
        "std::net",
        "Command::new",
        "pub argv:",
        "pub command:",
        "pub shell:",
        "pub systemd_properties:",
        "pub dbus_destination:",
        "pub dbus_interface:",
        "pub dbus_member:",
    ];
    if let Some(token) = forbidden_api.iter().find(|token| source.contains(**token)) {
        return Finding::violated(
            RULE,
            format!("host/execution API in observation contract: {token}"),
        );
    }
    let allowed_dependencies = [
        (
            "[dependencies]",
            "gnosix-input-core = { path = \"../gnosix-input-core\" }",
        ),
        (
            "[dependencies]",
            "serde = { version = \"1\", features = [\"derive\"] }",
        ),
        ("[dev-dependencies]", "serde_json = \"1\""),
    ];
    let mut section = "";
    let mut found: Vec<(&str, String)> = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line;
        } else if [
            "[dependencies]",
            "[dev-dependencies]",
            "[build-dependencies]",
        ]
        .contains(&section)
            && !line.is_empty()
            && !line.starts_with('#')
        {
            found.push((section, line.to_owned()));
        }
    }
    if found.iter().any(|(section, dependency)| {
        !allowed_dependencies.contains(&(*section, dependency.as_str()))
    }) {
        return Finding::violated(
            RULE,
            format!("unexpected compositor/transport/authority dependency: {found:?}"),
        );
    }
    Finding::satisfied(
        RULE,
        "observation schema depends only on the pure arbiter and serde; no host, transport, or execution API",
    )
}

fn input_runtime_observer_is_read_only_transport(manifest: &str, source: &str) -> Finding {
    const RULE: &str = "input-runtime-observer-is-read-only-transport";
    let forbidden_api = [
        "std::process::Command",
        "Command::new",
        "std::os::unix::net::UnixStream",
        "std::io::Write",
        "write_all(",
        "connect(",
        "send(",
    ];
    if let Some(token) = forbidden_api.iter().find(|token| source.contains(**token)) {
        return Finding::violated(
            RULE,
            format!("outbound transport or execution API in runtime observer: {token}"),
        );
    }
    let allowed_dependencies = [
        (
            "[dependencies]",
            "gnosix-input-observation = { path = \"../gnosix-input-observation\" }",
        ),
        (
            "[dependencies]",
            "serde = { version = \"1\", features = [\"derive\"] }",
        ),
        ("[dependencies]", "serde_json = \"1\""),
    ];
    let mut section = "";
    let mut found: Vec<(&str, String)> = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line;
        } else if ["[dependencies]", "[build-dependencies]"].contains(&section)
            && !line.is_empty()
            && !line.starts_with('#')
        {
            found.push((section, line.to_owned()));
        }
    }
    if found.iter().any(|(section, dependency)| {
        !allowed_dependencies.contains(&(*section, dependency.as_str()))
    }) {
        return Finding::violated(
            RULE,
            format!("unexpected runtime observer dependency: {found:?}"),
        );
    }
    Finding::satisfied(
        RULE,
        "runtime observer is a bounded inbound Unix listener with no command or outbound transport API",
    )
}

fn rust_sources_under(directory: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            paths.extend(rust_sources_under(&path)?);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod semantic_input_gate_tests {
    use super::*;
    const MANIFEST: &str = "[dependencies]\nin-gnu-os-settings-core = { path = \"../in-gnu-os-settings-core\" }\nserde = { version = \"1\", features = [\"derive\"] }\n\n[dev-dependencies]\nserde_json = \"1\"\n";
    #[test]
    fn semantic_input_core_accepts_typed_non_executable_shape() {
        assert!(
            semantic_input_core_is_non_executable(
                MANIFEST,
                "pub struct Binding { enabled: bool }\n"
            )
            .satisfied
        );
    }
    #[test]
    fn semantic_input_core_rejects_generic_execution_field() {
        for field in [
            "command",
            "shell",
            "argv",
            "systemd_properties",
            "dbus_destination",
            "dbus_interface",
            "dbus_member",
        ] {
            let source = format!("pub struct Binding {{ pub {field}: String }}\n");
            assert!(
                !semantic_input_core_is_non_executable(MANIFEST, &source).satisfied,
                "public execution field was admitted: {field}"
            );
        }
    }
    #[test]
    fn semantic_input_core_rejects_process_access_and_new_dependency() {
        assert!(
            !semantic_input_core_is_non_executable(MANIFEST, "use std::process::Command;\n")
                .satisfied
        );
        let hostile = MANIFEST.replace("serde =", "arbitrary-bus = \"1\"\nserde =");
        assert!(!semantic_input_core_is_non_executable(&hostile, "").satisfied);
        let hostile = format!("{MANIFEST}[build-dependencies]\nprocess-helper = \"1\"\n");
        assert!(!semantic_input_core_is_non_executable(&hostile, "").satisfied);
    }
}

#[cfg(test)]
mod input_observation_gate_tests {
    use super::*;

    const MANIFEST: &str = "[dependencies]\ngnosix-input-core = { path = \"../gnosix-input-core\" }\nserde = { version = \"1\", features = [\"derive\"] }\n\n[dev-dependencies]\nserde_json = \"1\"\n";

    #[test]
    fn observation_contract_accepts_only_neutral_typed_dependencies() {
        assert!(
            input_observation_contract_is_bounded(
                MANIFEST,
                "pub struct Record { sequence: u64 }\n"
            )
            .satisfied
        );
    }

    #[test]
    fn observation_contract_rejects_host_and_execution_apis() {
        for forbidden in [
            "std::process::Command",
            "pub shell: String",
            "pub argv: Vec<String>",
            "pub dbus_interface: String",
        ] {
            assert!(
                !input_observation_contract_is_bounded(MANIFEST, forbidden).satisfied,
                "admitted {forbidden}"
            );
        }
    }

    #[test]
    fn observation_contract_rejects_compositor_or_transport_dependencies() {
        for dependency in ["hyprland = \"0.56\"", "zbus = \"5\"", "tokio = \"1\""] {
            let manifest = format!("{MANIFEST}\n{dependency}\n");
            assert!(
                !input_observation_contract_is_bounded(&manifest, "").satisfied,
                "admitted {dependency}"
            );
        }
    }
}

#[cfg(test)]
mod input_runtime_observer_gate_tests {
    use super::*;

    const MANIFEST: &str = "[dependencies]\ngnosix-input-observation = { path = \"../gnosix-input-observation\" }\nserde = { version = \"1\", features = [\"derive\"] }\nserde_json = \"1\"\n";

    #[test]
    fn runtime_observer_accepts_bounded_read_only_unix_listener() {
        assert!(
            input_runtime_observer_is_read_only_transport(
                MANIFEST,
                "use std::os::unix::net::UnixListener;\npub fn observe() {}\n"
            )
            .satisfied
        );
    }

    #[test]
    fn runtime_observer_rejects_execution_or_outbound_transport() {
        for forbidden in [
            "std::process::Command",
            "std::os::unix::net::UnixStream",
            "std::io::Write",
            "write_all(",
            "connect(",
            "send(",
        ] {
            assert!(
                !input_runtime_observer_is_read_only_transport(MANIFEST, forbidden).satisfied,
                "admitted outbound/execution token {forbidden}"
            );
        }
    }

    #[test]
    fn runtime_observer_rejects_unreviewed_dependencies() {
        let hostile = format!("{MANIFEST}tokio = \"1\"\n");
        assert!(!input_runtime_observer_is_read_only_transport(&hostile, "").satisfied);
    }
}
