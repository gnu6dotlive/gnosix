//! Read-only inspection adapter. No imported source is evaluated or executed.
use gnosix_application_catalog::{
    ApplicationCatalog, ApplicationCatalogRoots, DesktopEntryId, DesktopEnvironment,
    LocalePreferences,
};
use gnosix_application_launch::{
    ApplicationLauncher, LaunchConfiguration, LaunchDocuments, LaunchReference,
};
use gnosix_input_core::{
    model::*,
    planner,
    resolution::{self, BrowserRoleResolution},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Serialize)]
struct RuntimeBind {
    evidence: &'static str,
    modmask: Option<i64>,
    key: Option<String>,
    keycode: Option<u64>,
    mouse: Option<bool>,
    release: Option<bool>,
    repeat: Option<bool>,
    long_press: Option<bool>,
    non_consuming: Option<bool>,
    auto_consuming: Option<bool>,
    locked: Option<bool>,
    catch_all: Option<bool>,
    submap: Option<String>,
    submap_universal: Option<String>,
    dispatcher: Option<String>,
    dispatcher_argument: Option<String>,
    typed_gesture: Option<Gesture>,
    inferred_semantic_mapping: Option<SemanticAction>,
    inference_limit: &'static str,
}
#[derive(Serialize)]
struct StaticBind {
    evidence: &'static str,
    source: String,
    line: usize,
    raw_gesture: Option<String>,
    typed_gesture: Option<Gesture>,
    callback_excerpt: String,
    mouse_option_text: bool,
    repeat_option_text: bool,
    locked_option_text: bool,
    release_option_text: bool,
    inferred_semantic_mapping: Option<SemanticAction>,
    interpretation: &'static str,
}
#[derive(Serialize)]
struct AssociationReport {
    association: BrowserAssociation,
    source: &'static str,
    raw_value: Option<String>,
    catalog_entry_found: Option<bool>,
    catalog_presentable: Option<bool>,
    launch_admission: Option<String>,
    observation: AssociationObservation,
}
#[derive(Serialize)]
struct RoleReport {
    semantic_role: &'static str,
    observations: Vec<AssociationReport>,
    resolution: BrowserRoleResolution,
    policy_scope: &'static str,
    catalog_identity: Option<String>,
    launch_admission: Option<String>,
}
#[derive(Serialize)]
struct ProviderFact {
    provider: String,
    capability: String,
    evidence: &'static str,
}
#[derive(Serialize)]
struct GraphRecord {
    evidence_class: &'static str,
    semantic_mapping_class: Option<&'static str>,
    physical_gesture: Value,
    raw_binding_indices: Vec<usize>,
    source_locations: Vec<String>,
    semantic_action: Option<SemanticAction>,
    provider_mechanism: String,
    status: &'static str,
}
#[derive(Serialize)]
struct ConflictSummary {
    semantic_candidates: Vec<String>,
    registered_gesture_candidates: Vec<resolution::ObservedGesturePairConflict>,
    effective_conflicts: &'static str,
}
#[derive(Serialize)]
struct Snapshot {
    schema: &'static str,
    observed_unix_seconds: u64,
    host: &'static str,
    hyprland: Value,
    catalog_generation: String,
    installed_application_count: usize,
    presentable_application_count: usize,
    catalog_issue_count: usize,
    runtime_bind_inventory: Vec<RuntimeBind>,
    static_candidates: Vec<StaticBind>,
    effective_precedence: &'static str,
    graph: Vec<GraphRecord>,
    browser_role: RoleReport,
    conflicts: ConflictSummary,
    providers: Vec<ProviderFact>,
    dry_run_examples: Vec<serde_json::Value>,
    limitations: Vec<&'static str>,
}

pub fn run(args: &[String]) -> ExitCode {
    let command = match validated_command(args) {
        Ok(c) => c,
        Err(()) => {
            eprintln!(
                "usage: cargo xtask keybindings [inspect|explain-super|explain-super-w|browser-role|dry-run] [--json]; APPLY_FORBIDDEN"
            );
            return ExitCode::from(2);
        }
    };
    match snapshot() {
        Ok(report) => {
            let output = match command {
                "explain-super" => {
                    json!({"query":"Super participation","runtime":report.runtime_bind_inventory.iter().filter(|b| b.modmask.is_some_and(|mask|mask & 64 != 0)).collect::<Vec<_>>(),"static_candidates":report.static_candidates.iter().filter(|b|b.raw_gesture.as_deref().is_some_and(|s|s.to_ascii_uppercase().contains("SUPER"))).collect::<Vec<_>>(),"finding":"registered Super modifier bindings exist; a release event alone does not establish isolated-tap arbitration"})
                }
                "explain-super-w" => {
                    json!({"query":"Super+W","runtime":report.runtime_bind_inventory.iter().filter(|b|b.key.as_deref()==Some("W")&&b.modmask==Some(64)).collect::<Vec<_>>(),"static_candidates":report.static_candidates.iter().filter(|b|b.typed_gesture.as_ref().is_some_and(is_super_w)).collect::<Vec<_>>(),"semantic_projection":"OpenDefaultWebBrowser","provider":"legacy literal executable priority callback (source evidence only)"})
                }
                "browser-role" => serde_json::to_value(&report.browser_role).unwrap_or(Value::Null),
                "dry-run" => json!(report.dry_run_examples),
                _ => serde_json::to_value(&report).unwrap_or(Value::Null),
            };
            match serde_json::to_string_pretty(&output) {
                Ok(s) => {
                    println!("{s}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("HARNESS_ERROR: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(e) => {
            eprintln!("HARNESS_ERROR: {e}");
            ExitCode::FAILURE
        }
    }
}
fn validated_command(args: &[String]) -> Result<&str, ()> {
    let command = args.first().map(String::as_str).unwrap_or("inspect");
    if args.iter().any(|a| a == "apply" || a == "--apply")
        || ![
            "inspect",
            "explain-super",
            "explain-super-w",
            "browser-role",
            "dry-run",
        ]
        .contains(&command)
        || args.iter().skip(1).any(|a| a != "--json")
    {
        Err(())
    } else {
        Ok(command)
    }
}
fn is_super_w(g: &Gesture) -> bool {
    matches!(g,Gesture::KeyChord{modifiers,key,..} if has_exact_modifiers(modifiers,&[Modifier::Super])&&key.as_str().eq_ignore_ascii_case("W"))
}
fn has_exact_modifiers(observed: &[Modifier], expected: &[Modifier]) -> bool {
    let mut observed = observed.to_vec();
    let mut expected = expected.to_vec();
    observed.sort();
    expected.sort();
    observed == expected
}
fn fixed_read(program: &str, args: &[&str]) -> Result<Value, String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{program} unavailable: {e}"))?;
    if !out.status.success() {
        return Err(format!("{program} {:?} exited {}", args, out.status));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("{program} returned invalid JSON: {e}"))
}
fn query(program: &str, args: &[&str]) -> AssociationObservation {
    match Command::new(program).args(args).output() {
        Ok(out) if out.status.success() => {
            let raw = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            match DesktopEntryId::parse(raw.clone()) {
                Ok(id) => AssociationObservation::Observed { id },
                Err(_) => AssociationObservation::Invalid {
                    raw,
                    detail: "not a valid desktop-entry identity".into(),
                },
            }
        }
        Ok(out) => AssociationObservation::Unavailable {
            detail: format!("exit status {}", out.status),
        },
        Err(e) => AssociationObservation::Unavailable {
            detail: e.to_string(),
        },
    }
}
fn snapshot() -> Result<Snapshot, String> {
    let version = fixed_read("hyprctl", &["-j", "version"])?;
    let binds = fixed_read("hyprctl", &["-j", "binds"])?;
    let runtime = binds
        .as_array()
        .ok_or("hyprctl binds JSON is not an array")?
        .iter()
        .map(parse_runtime)
        .collect::<Vec<_>>();
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME unavailable")?;
    let config = home.join(".config/hypr/hyprland.lua");
    let static_bindings = scan_sources(&config)?;
    let observations = vec![
        (
            BrowserAssociation::DefaultWebBrowser,
            "xdg-settings",
            vec!["get", "default-web-browser"],
        ),
        (
            BrowserAssociation::Http,
            "xdg-mime",
            vec!["query", "default", "x-scheme-handler/http"],
        ),
        (
            BrowserAssociation::Https,
            "xdg-mime",
            vec!["query", "default", "x-scheme-handler/https"],
        ),
        (
            BrowserAssociation::TextHtml,
            "xdg-mime",
            vec!["query", "default", "text/html"],
        ),
    ];
    let evidence = observations
        .iter()
        .map(|(kind, prog, args)| BrowserAssociationEvidence {
            association: *kind,
            observation: query(prog, args),
        })
        .collect::<Vec<_>>();
    let resolution = resolution::resolve_browser_role(&evidence);
    let browser_readiness =
        planner::role_readiness(SemanticAction::OpenDefaultWebBrowser, Some(&resolution));
    let roots = ApplicationCatalogRoots::from_environment()
        .map_err(|e| format!("catalog roots unavailable: {e:?}"))?;
    let catalog = ApplicationCatalog::discover(
        &roots,
        &LocalePreferences::from_environment(),
        DesktopEnvironment::from_environment(),
    );
    let catalog_generation = catalog.generation().hex();
    let installed_application_count = catalog.entries().len();
    let presentable_application_count = catalog.visible_entries().count();
    let catalog_issue_count = catalog.issues().len();
    let chosen = match &resolution {
        BrowserRoleResolution::Consistent { id } => Some(id.clone()),
        _ => None,
    };
    let launch_admission = chosen
        .as_deref()
        .and_then(|id| DesktopEntryId::parse(id).ok())
        .map(|id| {
            let request = LaunchReference::new(id, catalog.generation().clone());
            ApplicationLauncher::new(LaunchConfiguration::default())
                .plan(&request, &catalog, &LaunchDocuments::None)
                .map(|_| "admitted_by_bounded_launch_planner".to_owned())
                .unwrap_or_else(|e| format!("refused:{e:?}"))
        });
    let role_observations = observations
        .into_iter()
        .zip(evidence)
        .map(|((association, source, _), e)| {
            let (found, presentable, admission) = match &e.observation {
                AssociationObservation::Observed { id } => {
                    let entry = catalog.get(id);
                    let found = entry.is_some();
                    let presentable = entry.is_some_and(|a| {
                        a.normally_visible() && a.visible_in(catalog.desktop_environment())
                    });
                    let admission = entry.map(|_| {
                        let request =
                            LaunchReference::new(id.clone(), catalog.generation().clone());
                        ApplicationLauncher::new(LaunchConfiguration::default())
                            .plan(&request, &catalog, &LaunchDocuments::None)
                            .map(|_| "admitted".to_owned())
                            .unwrap_or_else(|err| format!("refused:{err:?}"))
                    });
                    (Some(found), Some(presentable), admission)
                }
                _ => (None, None, None),
            };
            AssociationReport {
                association,
                source,
                raw_value: match &e.observation {
                    AssociationObservation::Observed { id } => Some(id.as_str().to_owned()),
                    AssociationObservation::Invalid { raw, .. } => Some(raw.clone()),
                    _ => None,
                },
                catalog_entry_found: found,
                catalog_presentable: presentable,
                launch_admission: admission,
                observation: e.observation,
            }
        })
        .collect();
    let browser_role = RoleReport {
        semantic_role: "DefaultApplicationRole::WebBrowser",
        observations: role_observations,
        resolution,
        catalog_identity: chosen,
        launch_admission,
        policy_scope: "minimum inspected set: xdg-settings default-web-browser + MIME defaults http/https/text-html; no universal exhaustive cross-desktop browser-role set is established",
    };
    let mut graph = Vec::new();
    for (i, b) in runtime.iter().enumerate() {
        if let Some(action) = b.inferred_semantic_mapping {
            graph.push(GraphRecord {
                evidence_class: "OBSERVED_RAW_BINDING",
                semantic_mapping_class: Some("INFERRED_SEMANTIC_MAPPING"),
                physical_gesture: serde_json::to_value(&b.typed_gesture).unwrap_or(Value::Null),
                raw_binding_indices: vec![i],
                source_locations: Vec::new(),
                semantic_action: Some(action),
                provider_mechanism: b.dispatcher.clone().unwrap_or_else(|| "unknown".into()),
                status: "registered_runtime_inventory; not proof enabled/effective",
            });
        }
    }
    for b in &static_bindings {
        if let (Some(action), Some(gesture)) = (b.inferred_semantic_mapping, &b.typed_gesture) {
            graph.push(GraphRecord {
                evidence_class: "STATIC_SOURCE_CANDIDATE",
                semantic_mapping_class: Some("INFERRED_SEMANTIC_MAPPING"),
                physical_gesture: serde_json::to_value(gesture).unwrap_or(Value::Null),
                raw_binding_indices: Vec::new(),
                source_locations: vec![format!("{}:{}", b.source, b.line)],
                semantic_action: Some(action),
                provider_mechanism: "unexecuted Lua source candidate".into(),
                status: "static source only; not proved registered or effective",
            });
        }
    }
    let current = runtime
        .iter()
        .filter_map(runtime_to_binding)
        .collect::<Vec<_>>();
    let observed_gesture_candidates = runtime
        .iter()
        .enumerate()
        .filter_map(|(runtime_index, binding)| {
            Some(resolution::ObservedGestureCandidate {
                runtime_index,
                gesture: binding.typed_gesture.clone()?,
                semantic_action: binding.inferred_semantic_mapping,
                context: if binding.submap.as_deref().unwrap_or_default().is_empty() {
                    BindingContext::Global
                } else {
                    BindingContext::CompositorSubmap {
                        name: binding.submap.clone()?,
                    }
                },
            })
        })
        .collect::<Vec<_>>();
    let appstack_proposal = planner::sample_appstack_toggle();
    let appstack_sources = static_source_candidates_for(&appstack_proposal, &static_bindings);
    let browser_proposal = Binding {
        gesture: Gesture::KeyChord {
            modifiers: vec![Modifier::Super],
            key: KeyId::parse("W").expect("literal key"),
            trigger: KeyTrigger::Press,
        },
        semantic_action: SemanticAction::OpenDefaultWebBrowser,
        context: BindingContext::Global,
        provider: ProviderRef::DefaultApplicationRole {
            role: DefaultApplicationRole::WebBrowser,
        },
        enabled: true,
    };
    let browser_sources = static_source_candidates_for(&browser_proposal, &static_bindings);
    let dry_run_examples = vec![
        serde_json::to_value(planner::plan_with_observed_gestures(
            current.clone(),
            appstack_proposal,
            appstack_sources,
            planner::RoleReadiness::NotRelevant,
            &observed_gesture_candidates,
        ))
        .unwrap_or(Value::Null),
        serde_json::to_value(planner::plan_with_observed_gestures(
            current.clone(),
            browser_proposal,
            browser_sources,
            browser_readiness,
            &observed_gesture_candidates,
        ))
        .unwrap_or(Value::Null),
    ];
    let providers = vec![
        ProviderFact {
            provider: "GnosixAppStack".into(),
            capability: "ToggleApplicationList".into(),
            evidence: "implemented installed product capability; not currently bound to Super by 01A",
        },
        ProviderFact {
            provider: "InstalledDesktopApplication".into(),
            capability: "Launch".into(),
            evidence: "per-entry only when catalog identity and bounded planner admit; the inspector made no launch request",
        },
        ProviderFact {
            provider: "ExternalDesktopApplication".into(),
            capability: "Show / Hide / Toggle application list".into(),
            evidence: "UNVERIFIED; desktop launch admission is not a list-provider lifecycle capability",
        },
    ];
    let conflict_names = resolution::conflicts(&current)
        .into_iter()
        .map(|c| format!("{c:?}"))
        .collect();
    let observed_pair_conflicts =
        resolution::observed_gesture_pair_conflicts(&observed_gesture_candidates);
    let observed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(Snapshot {
        schema: "gnosix-keybindings-01a/v1",
        observed_unix_seconds: observed,
        host: "read_only_host_observation",
        hyprland: version,
        catalog_generation,
        installed_application_count,
        presentable_application_count,
        catalog_issue_count,
        runtime_bind_inventory: runtime,
        static_candidates: static_bindings,
        effective_precedence: "UNKNOWN from runtime binds JSON; registered inventory omits enabled/shadowed/device/source fields",
        graph,
        browser_role,
        conflicts: ConflictSummary {
            semantic_candidates: conflict_names,
            registered_gesture_candidates: observed_pair_conflicts,
            effective_conflicts: "UNKNOWN: binds JSON does not establish enabled/shadowed state, so these are candidate overlaps, not effective conflict verdicts",
        },
        providers,
        dry_run_examples,
        limitations: vec![
            "Lua callbacks are opaque __lua registry references; static source match is corroboration, not identity proof",
            "static Lua scanner records only literal hl.bind gesture arguments; unsupported/generated syntax is not executed",
            "hyprctl binds is registered inventory, not guaranteed enabled/effective bindings",
            "runtime binding JSON omits enabled, shadowed, device and source-line metadata",
            "no binding or MIME configuration was written; dry-run has no apply route",
        ],
    })
}
fn parse_runtime(v: &Value) -> RuntimeBind {
    let getstr = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
    let getbool = |k: &str| v.get(k).and_then(Value::as_bool);
    let modmask = v.get("modmask").and_then(Value::as_i64);
    let key = getstr("key");
    let release = getbool("release").unwrap_or(false);
    let gesture = key
        .as_deref()
        .and_then(|k| parse_gesture(k, modmask, release));
    let semantic = gesture.as_ref().and_then(infer_action);
    RuntimeBind {
        evidence: "OBSERVED_RAW_BINDING",
        modmask,
        key: key.clone(),
        keycode: v.get("keycode").and_then(Value::as_u64),
        mouse: getbool("mouse"),
        release: getbool("release"),
        repeat: getbool("repeat"),
        long_press: getbool("longPress"),
        non_consuming: getbool("non_consuming"),
        auto_consuming: getbool("auto_consuming"),
        locked: getbool("locked"),
        catch_all: getbool("catch_all"),
        submap: getstr("submap"),
        submap_universal: getstr("submap_universal"),
        dispatcher: getstr("dispatcher"),
        dispatcher_argument: getstr("arg"),
        typed_gesture: gesture,
        inferred_semantic_mapping: semantic,
        inference_limit: "modmask does not preserve left/right modifier identity; flags reflect reported JSON only",
    }
}
fn parse_gesture(key: &str, mask: Option<i64>, release: bool) -> Option<Gesture> {
    let mut mods = Vec::new();
    let m = mask?;
    if m & !(64 | 1 | 4 | 8) != 0 {
        return None;
    }
    if m & 64 != 0 {
        mods.push(Modifier::Super);
    }
    if m & 1 != 0 {
        mods.push(Modifier::Shift);
    }
    if m & 4 != 0 {
        mods.push(Modifier::Control);
    }
    if m & 8 != 0 {
        mods.push(Modifier::Alt);
    }
    if key.starts_with("mouse:") {
        let button = key.strip_prefix("mouse:")?.parse().ok()?;
        return Some(Gesture::PointerChord {
            modifiers: mods,
            button,
            trigger: if release {
                PointerTrigger::Release
            } else {
                PointerTrigger::Press
            },
        });
    }
    if key.starts_with("mouse_") {
        return None;
    }
    Some(Gesture::KeyChord {
        modifiers: mods,
        key: KeyId::parse(key).ok()?,
        trigger: if release {
            KeyTrigger::Release
        } else {
            KeyTrigger::Press
        },
    })
}
fn infer_action(g: &Gesture) -> Option<SemanticAction> {
    match g {
        Gesture::KeyChord { modifiers, key, .. }
            if has_exact_modifiers(modifiers, &[Modifier::Super])
                && key.as_str().eq_ignore_ascii_case("W") =>
        {
            Some(SemanticAction::OpenDefaultWebBrowser)
        }
        Gesture::KeyChord { modifiers, key, .. }
            if has_exact_modifiers(modifiers, &[Modifier::Super, Modifier::Shift])
                && key.as_str().eq_ignore_ascii_case("S") =>
        {
            Some(SemanticAction::CaptureScreenshot)
        }
        Gesture::PointerChord {
            modifiers,
            button: 272 | 274,
            ..
        } if has_exact_modifiers(modifiers, &[Modifier::Super]) => Some(SemanticAction::MoveWindow),
        Gesture::PointerChord {
            modifiers,
            button: 273,
            ..
        } if has_exact_modifiers(modifiers, &[Modifier::Super]) => {
            Some(SemanticAction::ResizeWindow)
        }
        _ => None,
    }
}
fn runtime_to_binding(b: &RuntimeBind) -> Option<Binding> {
    let action = b.inferred_semantic_mapping?;
    let provider = match action {
        SemanticAction::MoveWindow => ProviderRef::Compositor {
            capability: CompositorCapability::MoveWindow,
        },
        SemanticAction::ResizeWindow => ProviderRef::Compositor {
            capability: CompositorCapability::ResizeWindow,
        },
        _ => ProviderRef::UnresolvedObserved,
    };
    Some(Binding {
        gesture: b.typed_gesture.clone()?,
        semantic_action: action,
        context: if b.submap.as_deref().unwrap_or_default().is_empty() {
            BindingContext::Global
        } else {
            BindingContext::CompositorSubmap {
                name: b.submap.clone()?,
            }
        },
        provider,
        enabled: true,
    })
}

fn static_source_candidates_for(proposed: &Binding, static_bindings: &[StaticBind]) -> Vec<String> {
    let candidates = static_bindings
        .iter()
        .enumerate()
        .filter_map(|(index, source)| {
            Some(resolution::ObservedGestureCandidate {
                runtime_index: index,
                gesture: source.typed_gesture.clone()?,
                context: BindingContext::Global,
                semantic_action: source.inferred_semantic_mapping,
            })
        })
        .collect::<Vec<_>>();
    let overlaps = resolution::observed_gesture_conflicts(proposed, &candidates)
        .into_iter()
        .map(|candidate| candidate.runtime_index)
        .collect::<std::collections::BTreeSet<_>>();
    static_bindings
        .iter()
        .enumerate()
        .filter(|(index, _)| overlaps.contains(index))
        .map(|(_, source)| {
            format!(
                "{}:{} [STATIC_SOURCE_CANDIDATE]",
                source.source, source.line
            )
        })
        .collect()
}

fn scan_sources(root: &Path) -> Result<Vec<StaticBind>, String> {
    let mut todo = vec![root.to_path_buf()];
    let mut seen = std::collections::BTreeSet::new();
    let mut result = Vec::new();
    while let Some(path) = todo.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        for (line, call) in lua_calls(&text, "hl.bind") {
            let options = call_arguments(&call).get(2).copied().unwrap_or_default();
            let (raw, gesture) = first_literal_argument(&call).map_or((None, None), |s| {
                let parsed = parse_lua_gesture(&s, options.contains("release = true"));
                (Some(s), parsed)
            });
            let action = gesture.as_ref().and_then(infer_action);
            result.push(StaticBind{evidence:"STATIC_SOURCE_CANDIDATE",source:path.display().to_string(),line,raw_gesture:raw,typed_gesture:gesture,callback_excerpt:call.chars().take(480).collect(),mouse_option_text:options.contains("mouse = true"),repeat_option_text:options.contains("repeating = true"),locked_option_text:options.contains("locked = true"),release_option_text:options.contains("release = true"),inferred_semantic_mapping:action,interpretation:"source candidate only; runtime callback identity/effectiveness not proven"});
        }
        for (_line, call) in lua_calls(&text, "dofile") {
            let home = env::var_os("HOME").map(PathBuf::from);
            if let Some(include) = call_arguments(&call)
                .first()
                .and_then(|argument| {
                    dofile_include(argument, &path, home.as_deref().unwrap_or(Path::new("/")))
                })
                .filter(|p| p.is_file())
            {
                todo.push(include);
            }
        }
    }
    result.sort_by(|a, b| a.source.cmp(&b.source).then(a.line.cmp(&b.line)));
    Ok(result)
}
fn first_literal_argument(call: &str) -> Option<String> {
    literal_string(call_arguments(call).first()?.trim())
}
fn literal_string(expression: &str) -> Option<String> {
    let first = expression.trim();
    let bytes = first.as_bytes();
    let quote = *bytes.first()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i = (i + 2).min(bytes.len());
        } else if bytes[i] == quote {
            return Some(first[1..i].to_owned());
        } else {
            i += 1;
        }
    }
    None
}

fn dofile_include(argument: &str, source: &Path, home: &Path) -> Option<PathBuf> {
    let argument = argument.trim();
    if let Some(relative) = argument.strip_prefix("paths.home ..") {
        return Some(home.join(literal_string(relative)?.trim_start_matches('/')));
    }
    let literal = literal_string(argument)?;
    let path = PathBuf::from(literal);
    Some(if path.is_absolute() {
        path
    } else {
        source.parent().unwrap_or(Path::new(".")).join(path)
    })
}

/// Split literal top-level arguments without evaluating Lua. Nested callback
/// calls, tables, comments and strings cannot manufacture binding options.
fn call_arguments(call: &str) -> Vec<&str> {
    let bytes = call.as_bytes();
    let Some(open) = call.find('(') else {
        return Vec::new();
    };
    let mut arguments = Vec::new();
    let mut start = open + 1;
    let (mut parens, mut braces, mut brackets) = (1usize, 0usize, 0usize);
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] == b'-' && bytes.get(i + 1) == Some(&b'-') {
            i = comment_end(bytes, i);
            continue;
        }
        if let Some(end) = long_bracket_end(bytes, i) {
            i = end;
            continue;
        }
        if bytes[i] == b'\'' || bytes[i] == b'"' {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        match bytes[i] {
            b'(' => parens += 1,
            b')' => {
                parens -= 1;
                if parens == 0 {
                    let final_argument = call[start..i].trim();
                    if !final_argument.is_empty() {
                        arguments.push(final_argument);
                    }
                    break;
                }
            }
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b',' if parens == 1 && braces == 0 && brackets == 0 => {
                arguments.push(call[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    arguments
}
fn parse_lua_gesture(raw: &str, release: bool) -> Option<Gesture> {
    let (before, mods_text);
    if let Some((m, k)) = raw.split_once('+') {
        mods_text = Some(m);
        before = k.trim();
    } else {
        mods_text = None;
        before = raw.trim();
    }
    let mut modifiers = Vec::new();
    if let Some(m) = mods_text {
        let mut all = String::from(m);
        all.push('+');
        all.push_str(before);
        let parts = all
            .split('+')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        for p in &parts[..parts.len().saturating_sub(1)] {
            let m = match p.to_ascii_uppercase().as_str() {
                "SUPER" => Modifier::Super,
                "SHIFT" => Modifier::Shift,
                "CTRL" | "CONTROL" => Modifier::Control,
                "ALT" => Modifier::Alt,
                _ => return None,
            };
            if !modifiers.contains(&m) {
                modifiers.push(m);
            }
        }
        let key = parts.last()?.trim();
        if let Some(n) = key.strip_prefix("mouse:").and_then(|s| s.parse().ok()) {
            return Some(Gesture::PointerChord {
                modifiers,
                button: n,
                trigger: if release {
                    PointerTrigger::Release
                } else {
                    PointerTrigger::Press
                },
            });
        }
        return Some(Gesture::KeyChord {
            modifiers,
            key: KeyId::parse(key).ok()?,
            trigger: if release {
                KeyTrigger::Release
            } else {
                KeyTrigger::Press
            },
        });
    }
    Some(Gesture::KeyChord {
        modifiers,
        key: KeyId::parse(before).ok()?,
        trigger: if release {
            KeyTrigger::Release
        } else {
            KeyTrigger::Press
        },
    })
}
fn lua_calls(source: &str, name: &str) -> Vec<(usize, String)> {
    let b = source.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    let mut line = 1usize;
    while i < b.len() {
        if b[i] == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if b[i] == b'-' && b.get(i + 1) == Some(&b'-') {
            let end = comment_end(b, i);
            line += b[i..end].iter().filter(|c| **c == b'\n').count();
            i = end;
            continue;
        }
        if let Some(end) = long_bracket_end(b, i) {
            line += b[i..end].iter().filter(|c| **c == b'\n').count();
            i = end;
            continue;
        }
        if b[i] == b'\'' || b[i] == b'\"' {
            let q = b[i];
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    i = (i + 2).min(b.len());
                    continue;
                }
                if b[i] == q {
                    i += 1;
                    break;
                }
                if b[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            continue;
        }
        if source.get(i..).is_some_and(|s| s.starts_with(name))
            && source.as_bytes().get(i + name.len()) == Some(&b'(')
        {
            let start = i;
            let call_line = line;
            let mut j = i + name.len();
            let mut depth = 0i32;
            let mut quote = 0u8;
            let mut escape = false;
            while j < b.len() {
                let c = b[j];
                if c == b'\n' {
                    line += 1;
                }
                if quote != 0 {
                    if escape {
                        escape = false;
                    } else if c == b'\\' {
                        escape = true;
                    } else if c == quote {
                        quote = 0;
                    }
                    j += 1;
                    continue;
                }
                if let Some(end) = long_bracket_end(b, j) {
                    line += b[j..end].iter().filter(|c| **c == b'\n').count();
                    j = end;
                    continue;
                }
                if c == b'\'' || c == b'\"' {
                    quote = c;
                    j += 1;
                    continue;
                }
                if c == b'-' && b.get(j + 1) == Some(&b'-') {
                    let end = comment_end(b, j);
                    line += b[j..end].iter().filter(|c| **c == b'\n').count();
                    j = end;
                    continue;
                }
                if c == b'(' {
                    depth += 1;
                } else if c == b')' {
                    depth -= 1;
                    if depth == 0 {
                        j += 1;
                        break;
                    }
                }
                j += 1;
            }
            if depth == 0 {
                out.push((call_line, source[start..j].to_owned()));
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}
fn long_bracket_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut p = start + 1;
    while bytes.get(p) == Some(&b'=') {
        p += 1;
    }
    if bytes.get(p) != Some(&b'[') {
        return None;
    }
    let equals = p - start - 1;
    let mut q = p + 1;
    while q < bytes.len() {
        if bytes[q] == b']' {
            let mut e = q + 1;
            let mut n = 0;
            while bytes.get(e) == Some(&b'=') {
                n += 1;
                e += 1;
            }
            if n == equals && bytes.get(e) == Some(&b']') {
                return Some(e + 1);
            }
        }
        q += 1;
    }
    Some(bytes.len())
}
fn comment_end(bytes: &[u8], start: usize) -> usize {
    let body = (start + 2).min(bytes.len());
    if let Some(end) = long_bracket_end(bytes, body) {
        end
    } else {
        bytes[body..]
            .iter()
            .position(|b| *b == b'\n')
            .map_or(bytes.len(), |n| body + n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_inventory_does_not_claim_enabled_or_source_line() {
        let v = json!({"modmask":64,"key":"W","mouse":false,"release":false,"repeat":false,"dispatcher":"__lua","arg":"95","submap":""});
        let b = parse_runtime(&v);
        assert_eq!(
            b.inferred_semantic_mapping,
            Some(SemanticAction::OpenDefaultWebBrowser)
        );
        assert!(b.inference_limit.contains("left/right"));
        assert_eq!(b.dispatcher_argument.as_deref(), Some("95"));
    }
    #[test]
    fn static_scanner_ignores_comments_and_string_contents() {
        let s = "-- hl.bind(\"SUPER+X\")\nhl.bind(\"SUPER + W\", function() hl.exec_cmd([=[nested ) text]=]) end)";
        let found = lua_calls(s, "hl.bind");
        assert_eq!(found.len(), 1, "source={s:?}");
        assert_eq!(
            first_literal_argument(&found[0].1).as_deref(),
            Some("SUPER + W")
        );
    }
    #[test]
    fn static_options_come_only_from_the_top_level_options_argument() {
        let call =
            r#"hl.bind("SUPER+W", function() local text = 'release = true, mouse = true' end)"#;
        assert_eq!(first_literal_argument(call).as_deref(), Some("SUPER+W"));
        let args = call_arguments(call);
        assert_eq!(args.len(), 2);
        assert!(
            !args
                .get(2)
                .is_some_and(|options| options.contains("release = true"))
        );
        let with_options =
            "hl.bind(\"SUPER+W\", function() return f(1, 2) end, { release = true, mouse = true })";
        let args = call_arguments(with_options);
        assert_eq!(args.len(), 3);
        assert!(args[2].contains("release = true"));
        assert!(args[2].contains("mouse = true"));
        assert_eq!(
            first_literal_argument(r#"hl.bind(prefix .. "SUPER+W", callback)"#),
            None
        );
    }
    #[test]
    fn source_include_resolves_only_literal_paths_and_known_home_join() {
        let root = Path::new("/home/test/.config/hypr/hyprland.lua");
        let home = Path::new("/home/test");
        assert_eq!(
            dofile_include("paths.home .. \"/.config/hypr/gnusnip.lua\"", root, home),
            Some(PathBuf::from("/home/test/.config/hypr/gnusnip.lua"))
        );
        assert_eq!(
            dofile_include("\"included.lua\"", root, home),
            Some(PathBuf::from("/home/test/.config/hypr/included.lua"))
        );
        assert_eq!(dofile_include("resolve_config()", root, home), None);
    }
    #[test]
    fn static_super_w_maps_semantics_without_executing_callback() {
        let g = parse_lua_gesture("SUPER + W", false).unwrap();
        assert_eq!(
            infer_action(&g),
            Some(SemanticAction::OpenDefaultWebBrowser)
        );
    }
    #[test]
    fn semantic_inference_requires_the_exact_modifier_set() {
        let g = parse_lua_gesture("SUPER+SHIFT+W", false).unwrap();
        assert_eq!(infer_action(&g), None);
        let g = parse_lua_gesture("SUPER+CTRL+SHIFT+ALT+W", false).unwrap();
        assert_eq!(infer_action(&g), None);
    }
    #[test]
    fn pointer_mouse_option_does_not_change_key_derived_pointer_gesture() {
        let g = parse_lua_gesture("SUPER + mouse:272", false).unwrap();
        assert!(matches!(g, Gesture::PointerChord { button: 272, .. }));
    }
    #[test]
    fn unsupported_modifier_masks_and_lua_tokens_fail_closed() {
        assert!(parse_gesture("W", Some(64 | 16), false).is_none());
        assert!(parse_lua_gesture("SUPER+HYPER+W", false).is_none());
    }
    #[test]
    fn fixed_cli_rejects_apply_and_arbitrary_command_options() {
        assert!(validated_command(&["apply".into()]).is_err());
        assert!(validated_command(&["inspect".into(), "--root".into(), "/tmp".into()]).is_err());
        assert!(validated_command(&["inspect".into(), "--json".into()]).is_ok());
    }
}
