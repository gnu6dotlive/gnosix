//! Fixed execution boundary for already admitted plans. No configurable bus,
//! unit properties, environment, or executable constructor is exposed.

use crate::LaunchPlan;
use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zbus::{
    blocking::Proxy,
    proxy::CacheProperties,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchExecutionOwner {
    SystemdUserTransientService {
        unit: String,
    },
    /// Only controlled harnesses may use this ownership model. A direct child
    /// inherits its caller's cgroup and does not outlive supervised caller stop.
    ControlledHarness,
}

impl fmt::Display for LaunchExecutionOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SystemdUserTransientService { unit } => {
                write!(f, "systemd-user-transient:{unit}")
            }
            Self::ControlledHarness => f.write_str("controlled-harness"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchExecutionReceipt {
    pub pid: u32,
    pub owner: LaunchExecutionOwner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchExecutionError {
    UserManagerUnavailable(String),
    TransientUnitRejected(String),
    ExecutableStartFailed(String),
    MainPidUnavailable(String),
    InvalidPlanEncoding,
    UnitIdentityExhausted,
}

impl fmt::Display for LaunchExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for LaunchExecutionError {}

pub trait LaunchExecutor {
    fn execute(&self, plan: &LaunchPlan) -> Result<LaunchExecutionReceipt, LaunchExecutionError>;
}

/// Production executor: only StartTransientUnit and observation of that unit.
/// Session environment is inherited from the user manager, never copied from
/// the scene. No fallback to a direct child or an alternative bus exists.
///
/// A product cannot submit an executable or property list:
/// ```compile_fail
/// use gnosix_application_launch::{LaunchExecutor, SystemdUserLaunchExecutor};
/// SystemdUserLaunchExecutor.execute("/usr/bin/true", &["KillMode=process"]);
/// ```
/// Nor can it use the executor as a generic service manager:
/// ```compile_fail
/// use gnosix_application_launch::SystemdUserLaunchExecutor;
/// SystemdUserLaunchExecutor.start_unit("anything.service");
/// ```
/// A plan can only come from catalog resolution:
/// ```compile_fail
/// use gnosix_application_launch::LaunchPlan;
/// let plan = LaunchPlan { program: "/usr/bin/true".into(), args: vec![] };
/// ```
pub struct SystemdUserLaunchExecutor;

type Properties = Vec<(&'static str, OwnedValue)>;
type AuxiliaryUnits = Vec<(String, Properties)>;
type ExecCommands = Vec<(String, Vec<String>, Vec<String>)>;

fn properties(plan: &LaunchPlan) -> Result<Properties, LaunchExecutionError> {
    let argv: Vec<String> = plan
        .argv()
        .into_iter()
        .map(|arg| {
            arg.to_str()
                .filter(|s| !s.contains('\0'))
                .map(str::to_owned)
                .ok_or(LaunchExecutionError::InvalidPlanEncoding)
        })
        .collect::<Result<_, _>>()?;
    // ExecStartEx is ExecStart with explicit no-env-expand. Without that flag,
    // systemd could expand $VAR in an admitted argument even without a shell.
    // D-Bus supplies the argv directly; percent specifiers are not parsed here.
    // Preserve the old direct backend's PATH/current-directory lookup, instead
    // of letting systemd's compiled executable search choose another program.
    // argv[0] remains the admitted token. No environment is forwarded.
    let path = executable_path(plan.program())?;
    let commands: ExecCommands = vec![(path, argv, vec!["no-env-expand".into()])];
    let owned = |value: Value<'_>| {
        OwnedValue::try_from(value).map_err(|_| LaunchExecutionError::InvalidPlanEncoding)
    };
    Ok(vec![
        ("Type", owned(Value::from("exec"))?),
        ("CollectMode", owned(Value::from("inactive-or-failed"))?),
        ("ExecStartEx", owned(Value::from(commands))?),
    ])
}

fn executable_path(program: &std::ffi::OsStr) -> Result<String, LaunchExecutionError> {
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};
    let program_path = PathBuf::from(program);
    let candidates = if program_path.is_absolute() {
        vec![program_path]
    } else if program_path.components().count() > 1 {
        vec![
            std::env::current_dir()
                .map_err(|e| LaunchExecutionError::ExecutableStartFailed(e.to_string()))?
                .join(program_path),
        ]
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join(&program_path))
            .collect()
    };
    for candidate in candidates {
        if candidate
            .metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        {
            let absolute = if candidate.is_absolute() {
                candidate
            } else {
                std::env::current_dir()
                    .map_err(|e| LaunchExecutionError::ExecutableStartFailed(e.to_string()))?
                    .join(candidate)
            };
            return absolute
                .to_str()
                .map(str::to_owned)
                .ok_or(LaunchExecutionError::InvalidPlanEncoding);
        }
    }
    Err(LaunchExecutionError::ExecutableStartFailed(
        "admitted executable not found or not executable".into(),
    ))
}

fn next_unit() -> Result<String, LaunchExecutionError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| LaunchExecutionError::UnitIdentityExhausted)?;
    Ok(format!("gnosix-app-{}-{n}.service", std::process::id()))
}

fn job_pending(observation: zbus::Result<String>) -> zbus::Result<bool> {
    match observation {
        Ok(_) => Ok(true),
        // Property reads go through the FDO Properties proxy, so an absent
        // completed job can arrive wrapped as FDO rather than MethodError.
        Err(zbus::Error::FDO(error)) if matches!(*error, zbus::fdo::Error::UnknownObject(_)) => {
            Ok(false)
        }
        Err(zbus::Error::MethodError(name, _, _))
            if matches!(
                name.as_str(),
                "org.freedesktop.DBus.Error.UnknownObject" | "org.freedesktop.systemd1.NoSuchJob"
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

impl LaunchExecutor for SystemdUserLaunchExecutor {
    fn execute(&self, plan: &LaunchPlan) -> Result<LaunchExecutionReceipt, LaunchExecutionError> {
        let props = properties(plan)?;
        let unit = next_unit()?;
        let unavailable =
            |e: zbus::Error| LaunchExecutionError::UserManagerUnavailable(format!("{unit}: {e}"));
        let connection = zbus::blocking::connection::Builder::session()
            .map_err(unavailable)?
            .method_timeout(Duration::from_secs(5))
            .build()
            .map_err(unavailable)?;
        let manager = Proxy::new(
            &connection,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )
        .map_err(unavailable)?;
        let job_path: OwnedObjectPath = manager
            .call(
                "StartTransientUnit",
                &(&unit, "fail", props, AuxiliaryUnits::new()),
            )
            .map_err(|e| match &e {
                zbus::Error::MethodError(name, _, _)
                    if !matches!(
                        name.as_str(),
                        "org.freedesktop.DBus.Error.ServiceUnknown"
                            | "org.freedesktop.DBus.Error.NameHasNoOwner"
                    ) =>
                {
                    LaunchExecutionError::TransientUnitRejected(e.to_string())
                }
                _ => unavailable(e),
            })?;
        let path: OwnedObjectPath = manager
            .call("GetUnit", &(&unit,))
            .map_err(|e| LaunchExecutionError::MainPidUnavailable(format!("{unit}: {e}")))?;
        let proxy = |interface: &'static str| -> zbus::Result<Proxy<'_>> {
            zbus::blocking::proxy::Builder::new(&connection)
                .destination("org.freedesktop.systemd1")?
                .path(path.as_str())?
                .interface(interface)?
                .cache_properties(CacheProperties::No)
                .build()
        };
        let service = proxy("org.freedesktop.systemd1.Service").map_err(unavailable)?;
        let state = proxy("org.freedesktop.systemd1.Unit").map_err(unavailable)?;
        let job: Proxy<'_> = zbus::blocking::proxy::Builder::new(&connection)
            .destination("org.freedesktop.systemd1")
            .map_err(unavailable)?
            .path(job_path.as_str())
            .map_err(unavailable)?
            .interface("org.freedesktop.systemd1.Job")
            .map_err(unavailable)?
            .cache_properties(CacheProperties::No)
            .build()
            .map_err(unavailable)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let result: String = service.get_property("Result").map_err(unavailable)?;
            if result != "success" {
                return Err(LaunchExecutionError::ExecutableStartFailed(format!(
                    "{unit}: {result}"
                )));
            }
            let pid: u32 = service.get_property("MainPID").map_err(unavailable)?;
            let active: String = state.get_property("ActiveState").map_err(unavailable)?;
            // Type=exec becomes active only after successful exec, not fork or
            // job submission. Never report a request accepted with a zero PID.
            if active == "active" && pid > 0 {
                return Ok(LaunchExecutionReceipt {
                    pid,
                    owner: LaunchExecutionOwner::SystemdUserTransientService { unit },
                });
            }
            // A newly queued start can still be inactive. Do not report failure
            // while its job is waiting/running and then launch it afterwards.
            let pending = job_pending(job.get_property::<String>("State")).map_err(unavailable)?;
            if (!pending && matches!(active.as_str(), "failed" | "inactive"))
                || Instant::now() >= deadline
            {
                return Err(LaunchExecutionError::MainPidUnavailable(format!(
                    "{unit}: {active}"
                )));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn plan() -> LaunchPlan {
        LaunchPlan {
            entry: crate::DesktopEntryId::parse("probe.desktop").unwrap(),
            generation: gnosix_application_catalog::ApplicationCatalog::empty()
                .generation()
                .clone(),
            program: OsString::from("/usr/bin/printf"),
            args: vec![
                "literal space".into(),
                "$HOME".into(),
                "%n".into(),
                "semi;colon".into(),
            ],
            substituted_field_codes: Default::default(),
            via_terminal: false,
        }
    }

    #[test]
    fn transient_contract_is_exact_and_cannot_expand_arguments() {
        let plan = plan();
        let mut props = properties(&plan).unwrap();
        assert_eq!(
            props.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            ["Type", "CollectMode", "ExecStartEx"]
        );
        assert_eq!(String::try_from(props.remove(0).1).unwrap(), "exec");
        assert_eq!(
            String::try_from(props.remove(0).1).unwrap(),
            "inactive-or-failed"
        );
        let commands = ExecCommands::try_from(props.remove(0).1).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].0, "/usr/bin/printf");
        assert_eq!(
            commands[0].1,
            plan.argv()
                .iter()
                .map(|s| s.to_str().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(commands[0].2, ["no-env-expand"]);
    }

    #[test]
    fn invalid_encoding_fails_before_contacting_the_manager() {
        use std::os::unix::ffi::OsStringExt;
        let mut plan = plan();
        for arg in [OsString::from_vec(vec![0xff]), OsString::from("nul\0byte")] {
            plan.args = vec![arg];
            assert_eq!(
                properties(&plan).unwrap_err(),
                LaunchExecutionError::InvalidPlanEncoding
            );
        }
    }

    #[test]
    fn execution_names_are_bounded_unique_and_not_application_labels() {
        let a = next_unit().unwrap();
        let b = next_unit().unwrap();
        assert_ne!(a, b);
        assert!(a.len() < 80);
        assert!(a.starts_with("gnosix-app-") && a.ends_with(".service"));
        assert!(
            a.chars()
                .all(|c| c.is_ascii_digit() || "gnosixap-.ervc".contains(c))
        );
    }

    #[test]
    fn completed_job_is_not_mistaken_for_manager_transport_failure() {
        assert!(job_pending(Ok("waiting".into())).unwrap());
        assert!(job_pending(Ok("running".into())).unwrap());
        assert!(
            !job_pending(Err(zbus::Error::FDO(Box::new(
                zbus::fdo::Error::UnknownObject("collected job".into())
            ))))
            .unwrap()
        );
        assert!(
            job_pending(Err(zbus::Error::FDO(Box::new(
                zbus::fdo::Error::AccessDenied("denied".into())
            ))))
            .is_err()
        );
    }
}
