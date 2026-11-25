# Security policy

TotalFloat parses untrusted schedule files and will run as a server in team deployments. We take reports seriously.

## Reporting a vulnerability

Please report privately through GitHub's "Report a vulnerability" (Security → Advisories) on this repository. Don't open a public issue. We aim to acknowledge reports within 7 days.

## Scope

- Crashes, hangs or excessive resource use when parsing XER, XML or other schedule files.
- Any path where schedule content can cause code execution, file access, or network access.
- In server mode (once it exists): authentication, authorization, and data isolation issues.

## Commitments

- The core makes no network calls and collects no telemetry.
- Releases will be signed and ship with an SBOM.
