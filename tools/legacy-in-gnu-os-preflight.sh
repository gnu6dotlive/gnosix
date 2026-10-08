#!/bin/sh
# Compatibility guard for the exactly identified, deprecated in.gnu.os binary.
# The historical binary generates an UNSCOPED Tetragon policy at startup.
# It may run only while the tetra CLI is unavailable in the service's PATH.
# This guard IS NOT sandbox enforcement. Do not generalize it to new binaries.
set -eu
expected_sha256='44cb6461c9f025923098485bd7a5a7550e0da9018b6c63de5778c1482c66e7e7'
engine="${HOME}/.local/bin/in.gnu.os"
refuse() {
    printf 'GNOSIX_LEGACY_GUARD_DENIED: %s\n' "$1" >&2
    exit 1
}
[ -f "$engine" ] && [ -x "$engine" ] || refuse "legacy executable missing or not executable"
actual_sha256="$(sha256sum "$engine" | cut -d ' ' -f 1)" || refuse "cannot hash legacy executable"
[ "$actual_sha256" = "$expected_sha256" ] || refuse "binary changed; re-audit before startup"
if command -v tetra >/dev/null 2>&1; then
    refuse "tetra available: historical engine could install unscoped policy"
fi
printf 'GNOSIX_LEGACY_GUARD_OK: known binary, tetra unavailable; Tetragon enforcement NOT active\n'
