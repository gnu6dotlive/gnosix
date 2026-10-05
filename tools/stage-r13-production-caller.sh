#!/usr/bin/env bash
# Default: build authority and caller from this checkout.
# Caller-only: GNOSIX_R13_AUTHORITY_ARTIFACT=/absolute/ratified/artifact
# carries forward the pinned authority while building the current caller.
# The resulting manifest describes composite bytes, not one source revision;
# promotion evidence must record each component's separate provenance.
set -euo pipefail

if [[ -z "${DESTDIR:-}" || "$DESTDIR" == "/" ]]; then
    echo "DESTDIR must name an isolated staging root; live-root installation is forbidden" >&2
    exit 2
fi
if [[ "$DESTDIR" != /* ]]; then
    echo "DESTDIR must be an absolute isolated staging root" >&2
    exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Unset retains the historical build-both workflow. An explicitly supplied
# artifact selects caller-only promotion; even an empty value must fail closed.
# Its provenance is a previously ratified binary, NOT current-source authority.
authority_snapshot=""
if [[ -v GNOSIX_R13_AUTHORITY_ARTIFACT ]]; then
    artifact="$GNOSIX_R13_AUTHORITY_ARTIFACT"
    if [[ "$artifact" != /* || ! -f "$artifact" || ! -x "$artifact" ]]; then
        echo "frozen authority must be an absolute regular executable file" >&2
        exit 2
    fi
    expected_authority_sha256="$(cat "$repo_root/tools/r13-authority.sha256")"
    if [[ ! "$expected_authority_sha256" =~ ^[0-9a-f]{64}$ ]]; then
        echo "invalid canonical authority digest" >&2
        exit 2
    fi
    # Freeze bytes before a potentially long caller build. Stage and verify
    # this same snapshot, so a changing supplied path cannot substitute bytes.
    authority_snapshot="$(mktemp)"
    trap 'rm -f -- "$authority_snapshot"' EXIT
    cp -- "$artifact" "$authority_snapshot"
    snapshot_digest="$(sha256sum "$authority_snapshot")"
    if [[ "${snapshot_digest%% *}" != "$expected_authority_sha256" ]]; then
        echo "frozen authority does not match the canonical digest" >&2
        exit 2
    fi
else
    cargo build --manifest-path "$repo_root/Cargo.toml" \
        -p gnosix-hyprland \
        --release \
        --features r13-production-dbus \
        --bin in-gnu-os-activationd
fi

cargo build --manifest-path "$repo_root/Cargo.toml" \
    -p gnos-ux-gpui \
    --release \
    --features r13-production-client \
    --bin gnos-ux-scene

target_dir="$(cargo metadata --manifest-path "$repo_root/Cargo.toml" --no-deps --format-version 1 \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"

authority_source="${authority_snapshot:-$target_dir/release/in-gnu-os-activationd}"
install -D -m 0755 "$authority_source" \
    "$DESTDIR/usr/libexec/gnosix/in-gnu-os-activationd"
if [[ -n "$authority_snapshot" ]]; then
    staged_digest="$(sha256sum "$DESTDIR/usr/libexec/gnosix/in-gnu-os-activationd")"
    if [[ "${staged_digest%% *}" != "$expected_authority_sha256" ]]; then
        echo "staged frozen authority does not match the canonical digest" >&2
        exit 2
    fi
    cmp -- "$authority_snapshot" "$DESTDIR/usr/libexec/gnosix/in-gnu-os-activationd"
fi
install -D -m 0755 "$target_dir/release/gnos-ux-scene" \
    "$DESTDIR/usr/libexec/gnosix/gnos-ux-scene"

install -D -m 0644 "$repo_root/systemd/user/in-gnu-os-activation.service" \
    "$DESTDIR/usr/lib/systemd/user/in-gnu-os-activation.service"
install -D -m 0644 "$repo_root/systemd/user/gnos-ux-scene.service" \
    "$DESTDIR/usr/lib/systemd/user/gnos-ux-scene.service"
install -D -m 0644 "$repo_root/dbus-1/services/in.gnu.os.Activation1.service" \
    "$DESTDIR/usr/share/dbus-1/services/in.gnu.os.Activation1.service"

manifest="$DESTDIR/usr/share/gnosix/manifests/r13-production-caller.sha256"
mkdir -p "$(dirname "$manifest")"
(
    cd "$DESTDIR"
    sha256sum \
        usr/libexec/gnosix/in-gnu-os-activationd \
        usr/libexec/gnosix/gnos-ux-scene \
        usr/lib/systemd/user/in-gnu-os-activation.service \
        usr/lib/systemd/user/gnos-ux-scene.service \
        usr/share/dbus-1/services/in.gnu.os.Activation1.service
) > "$manifest"

printf 'staged R13 authority binary: %s\n' "$DESTDIR/usr/libexec/gnosix/in-gnu-os-activationd"
printf 'staged R13 caller binary: %s\n' "$DESTDIR/usr/libexec/gnosix/gnos-ux-scene"
printf 'staged R13 authority unit: %s\n' "$DESTDIR/usr/lib/systemd/user/in-gnu-os-activation.service"
printf 'staged R13 caller unit: %s\n' "$DESTDIR/usr/lib/systemd/user/gnos-ux-scene.service"
printf 'staged R13 D-Bus activation descriptor: %s\n' "$DESTDIR/usr/share/dbus-1/services/in.gnu.os.Activation1.service"
printf 'staged R13 manifest: %s\n' "$manifest"
