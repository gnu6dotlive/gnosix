# Security policy

## Project status

gnosix is experimental / pre-release. Security-sensitive boundaries are designed and tested explicitly, but no stable end-user security certification or release guarantee is claimed.

## Reporting a vulnerability

Do not post exploit details, secrets, authentication bypasses, privilege-escalation paths, or other sensitive findings in a public issue.

When GitHub private vulnerability reporting is available for this repository, use it. If it is unavailable, use the private contact method published by the maintainer profile and disclose only enough information to establish a secure follow-up channel.

## Security-sensitive areas

Changes deserve additional scrutiny when they touch:

- caller/process authentication and replay protection;
- D-Bus or other cross-process boundaries;
- profile/identity persistence and recovery;
- activation, process launch, or systemd user units;
- compositor mutation or input observation;
- secrets, credentials, lock/authentication surfaces, or privilege escalation;
- parsing of untrusted desktop/application/provider data.

## Expectations

Fail closed when caller identity, freshness, target identity, or state provenance cannot be established. Keep read/observe paths separate from mutation capabilities. Do not widen authority merely to make a test fixture or integration convenient.

Security fixes should include a regression that demonstrates the previous unsafe behavior when practical, then proves the corrected boundary without erasing the original finding.
