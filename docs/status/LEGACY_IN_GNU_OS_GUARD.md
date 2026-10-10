# Legacy in.gnu.os containment — 2026-10-08

**State:** compatibility mitigation, **NOT** Tetragon enforcement; does not retire the engine. The canonical Gnosix activation authority remains separate. Public product status is unchanged.

## Why

A pre-modern standalone `in.gnu.os` binary remains enabled on one operator workstation, registering `org.gnosis.Engine` and `org.freedesktop.impl.portal.desktop.gnosis` on the user D-Bus. The code that originally built it is available only as an old, non-Git snapshot, not as a current release source in this repository. Do **not** replace it from that snapshot without migrating consumers and validating a new owner.

Its startup attempts `tetra tracingpolicy add /tmp/tetragon-gnosis-agent-sandbox.yaml`. The historical `generate_policy_yaml(&self, _cgroup_slice, risk)` **ignores** the declared cgroup target. The generated `sys_setuid` and `sys_connect` hooks specify enforcement actions with no PID, binary, namespace, or workload selectors; `sys_openat` only filters arguments. This is **not** a scope-limited MCP sandbox. The external `tetra` program is currently absent, so the application attempt fails and the daemon logs a warning. Do not install or enable Tetragon to quiet that warning.

Other legacy warnings are source-explained, not fixed: the installed binary lacks its `llm` Cargo feature, and it opens `/gnosis_shm` even though the old Hyprland producer plugin is not loaded. Neither failure means the current Rust-native Gnosix activation product is failing; these are historical runtime interfaces needing explicit retire/migrate decisions.

## Compatibility guard

`tools/legacy-in-gnu-os-preflight.sh` is pinned to the SHA-256 digest of the **observed installed binary**, `44cb6461c9f025923098485bd7a5a7550e0da9018b6c63de5778c1482c66e7e7`. Before **new** launches of that legacy service, the guard:
1. Requires an executable at `$HOME/.local/bin/in.gnu.os`.
2. Requires the byte-exact pinned digest. Any new binary must be reviewed before launch.
3. Requires `tetra` to be absent from the inherited executable search path; otherwise declines the launch so the historical global policy cannot be installed by that command.
4. Emits `Tetragon enforcement NOT active` on success. It does **not** imply isolation or monitor any already-running instance.

`systemd/user/in.gnu.os.service.d/90-legacy-guard.conf` adds the `ExecCondition` without replacing the human-owned base unit. The drop-in also orders after `graphical-session.target`. The base unit must retain `ConditionEnvironment=HYPRLAND_INSTANCE_SIGNATURE`, `PartOf=graphical-session.target`, and `WantedBy=graphical-session.target`.

**Limits:** a `tetra` binary appearing after startup is not detected until a subsequent start; race conditions and other security policies remain outside this gate. `ExecCondition` returning 1-254 skips startup without marking the unit failed. An externally disabled `tetra` path does not prove any Tetragon enforcement state. This is a narrow temporary fail-closed safeguard for one known binary, not a security architecture.

## Operator deployment (user service only)

After source tests, confirm the legacy user service and binary are the expected ones, then stage the tracked files, preserving existing unrelated configuration:

```sh
python3 -m unittest -v tools/test_legacy_in_gnu_os_preflight.py
install -D -m 0755 tools/legacy-in-gnu-os-preflight.sh \
  "$HOME/.local/libexec/gnosix/legacy-in-gnu-os-preflight"
install -D -m 0644 systemd/user/in.gnu.os.service.d/90-legacy-guard.conf \
  "$HOME/.config/systemd/user/in.gnu.os.service.d/90-legacy-guard.conf"
systemd-analyze --user verify "$HOME/.config/systemd/user/in.gnu.os.service"
systemctl --user daemon-reload
"$HOME/.local/libexec/gnosix/legacy-in-gnu-os-preflight"
systemctl --user restart in.gnu.os.service
systemctl --user show in.gnu.os.service \
  -p ActiveState -p SubState -p ConditionResult -p ExecCondition -p MainPID
```

If existing destination files differ, stop and reconcile rather than overwriting without a retained backup. The guard is for the **user unit**, not for the standalone `in-gnu-os-activation.service`. It does not require root or host package installation.

For rollback of just this guard, preserve a copy of both deployed files, remove **only** the new `90-legacy-guard.conf` drop-in, run `systemctl --user daemon-reload`, then restart `in.gnu.os.service` only if the restored configuration is verified. This rollback removes a safety rail and should not be performed merely to run the historical process while `tetra` is installed.

## Deferred migration gates

- Prove which DBus clients still depend on the old `org.gnosis.Engine` / portal paths and `gnu6.sock`; migrate them before retiring the engine.
- Replace legacy scanner with explicitly selected modeld/local inference contract (no automatic `llm` build).
- Retire `/gnosis_shm` or implement a separately qualified, versioned producer-consumer transport; no unreviewed Hyprland plugin loading.
- Design scoped enforcement against the real MCP execution identity; validate isolated positive/negative cases, rollback, and explicit activation receipts. Never adopt this legacy policy YAML.
- Cold boot and compositor A→B qualification: current-session and static UWSM ordering proof is not sufficient to claim this is closed.

Local workstation evidence: `/mnt/workbench/evidence/runs/in-gnu-os-lifecycle-20261008/`.
