# Security Policy

## Supported Versions

Fluxion is currently pre-release software. Security fixes are applied to the latest `main` branch; older snapshots are not supported.

## Reporting A Vulnerability

Please report vulnerabilities privately through the repository's **Security** tab:

1. Open **Security**.
2. Select **Advisories**.
3. Choose **Report a vulnerability**.

If private vulnerability reporting is not enabled yet, contact a maintainer before sharing details. Do not open a public issue containing exploit steps, credentials, private URLs, user data, or signing material.

Include the affected version or commit, operating system, impact, minimal reproduction steps, and any suggested mitigation. You should receive an acknowledgement within seven days. We will coordinate remediation and public disclosure based on severity and release availability.

## Scope Notes

High-value areas include credential storage and redaction, URL and header handling, download-path containment, destructive file operations, SFTP host-key verification, updater signatures, daemon IPC, and browser handoff code.
