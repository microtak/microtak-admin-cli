# Security policy

microtak-admin-cli holds a MicroTAK server's admin certificate and key,
which can enroll, revoke and reassign every device and mission on that
server. Security reports are very welcome.

## Supported versions

Fixes go into the latest release only, published as a patch release.
Please upgrade to the latest release before reporting.

## Reporting a vulnerability

**Please don't open a public issue.** Report privately through GitHub:
[Security → Report a vulnerability](https://github.com/microtak/microtak-admin-cli/security/advisories/new).

Helpful to include the version, how you run it, what an attacker needs,
and steps or a proof of concept. Reports are handled as soon as possible;
this is a small project without a formal response-time commitment. You'll
get credit in the advisory unless you prefer not to.

Vulnerabilities in microtak-server itself go to
[microtak-server](https://github.com/microtak/microtak-server/security/advisories/new).

## Scope and hardening

In scope: this tool's handling of credentials and server responses.
Protecting the admin key files on your own machine is up to you; keep
them owner-only (`chmod 600`).

- `enroll` writes the private key owner-only (0600 on Unix).
- The server is always verified, against `--ca` or the system roots;
  `http://` URLs are refused, and there is no option to skip
  verification.
- Dependencies are scanned with OSV-Scanner in CI.
