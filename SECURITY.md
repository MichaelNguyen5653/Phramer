# Phramer Security Policy

## Supported Versions

Only the **latest stable release** of Phramer, or the **HEAD of the master
branch**, is actively supported with security updates. If you discover a
vulnerability, please verify that it persists in the current version before
reporting.

Phramer is a fork and carries upstream code. A flaw that originates upstream
and is still present there should also be reported to that project, so users
of both are protected.

## Reporting a Vulnerability

Please report vulnerabilities privately through
[GitHub's security advisory form](https://github.com/MichaelNguyen5653/Phramer/security/advisories/new).
Do not open a public issue for a security disclosure.

Please avoid framing a non-security bug as a security bug. If you are not sure
whether something is a security flaw, report it privately anyway and we will
work it out together.

## Report Requirements

To help triage and resolve the issue efficiently, your report should include:

1. **Reproducible instructions** — clear, step-by-step directions for
   replicating the vulnerability
2. **Proof of concept** — functional code or a script demonstrating the
   exploit
3. **Affected version** — the release or commit you observed it on, and your
   Windows version
4. If you used any software or AI to detect the vulnerability, please disclose
   that transparently

A [CVSS vector](https://nvd.nist.gov/vuln-metrics/cvss) is welcome but not
required.

**Optional**: suggestions, patches, or code fixes addressing the issue are
highly appreciated.

## Out-of-Scope Vulnerabilities

The following are considered out of scope unless they present a novel or
unique threat vector:

- Vulnerabilities that require prior administrator access or an
  already-compromised operating system. If administrator access is already
  compromised, a bad actor can do far worse than anything reachable through a
  screenshot tool.
- Theoretical issues, or raw output from automated security scanners, without
  a verified and functional proof of concept.

## Response Process

1. **Acknowledgment** — receipt of the report is confirmed.
2. **Triage** — the finding is validated using your reproduction steps.
3. **Fix and advisory** — if verified, a mitigation is prepared and a release
   is coordinated alongside a public security advisory.

## Commitments

- Transparency, where appropriate and applicable
- Evaluation and validation of reported potential vulnerabilities
- Acting on reports as quickly as is practical

Phramer is maintained in spare time, so triage and patching may take longer
than commercial software standards.

## Reporters' Responsibilities

- Provide accurate and sufficiently detailed information
- Be honest; avoid exaggerating or downplaying impact
- Maintain civil and professional etiquette throughout the process
