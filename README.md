# microtak-admin-cli

A command-line tool for managing a [MicroTAK](https://github.com/microtak/microtak-server)
server: device enrollment, enrollment invite tokens, and mission roles —
wraps the HTTP endpoints documented in `microtak-server`'s own
`docs/ARCHITECTURE.md` ("Enrollment lockdown / admin API" and "Mission
roles" sections) so you don't have to hand-craft `curl`/mTLS invocations
for routine access management.

## Building

```sh
cargo build --release
cargo test
```

The binary is named `microtak-admin-cli`.

`scripts/build.sh` runs the same build/test/clippy steps CI does (add
`--nix` to also run `nix flake check`); see `scripts/build.sh --help`.

## Enrolling a device

Talks to the server's enrollment endpoint — **HTTPS only**, no client
certificate (the device has none yet) — generates a keypair and CSR,
submits it, and saves the signed certificate and key. The endpoint is
verified against `--ca`, the server's own CA certificate (`ca-cert.pem` in
its data directory); leave `--ca` out only if the endpoint has a
publicly-trusted certificate (e.g. Let's Encrypt). Verification is never
skipped, and `http://` URLs are refused.

```sh
microtak-admin-cli enroll \
  --enrollment-url https://microtak.example.com:8446 --ca ca.pem \
  --cn my-device --token <invite token> \
  --out-cert my-device.pem --out-key my-device.key
```

By default (`enrollment_mode = "auto"`) the server is locked from its first
start, so every device needs an invite token (see below for minting one) —
except the admin device, which enrolls once with the server's one-time
**bootstrap token**. The server writes that token, next to its CA
certificate, to its data directory on first start:

```sh
# with Docker:
docker compose exec microtak-server cat data/bootstrap-token
docker compose exec microtak-server cat data/ca-cert.pem > ca.pem

microtak-admin-cli enroll \
  --enrollment-url https://microtak.example.com:8446 --ca ca.pem \
  --cn admin --token "$(cat bootstrap-token)" \
  --out-cert admin.pem --out-key admin.key
```

The URL's host name must be one the server certificate names — its
`server_common_name` (`microtak-server`), its interface addresses, or its
configured `server_names`. To connect by an address the certificate doesn't
name, use `--resolve` with a name it does, e.g.
`--enrollment-url https://microtak-server:8446 --resolve
microtak-server:8446:192.168.1.10`. `--out-ca ca.pem` saves the CA returned
in the enrollment response, for the admin commands' `--ca`.

`--cn` must match the server's `admin_common_name` (`"admin"` by default).
A token can only create a *new* identity; re-enrolling an existing one
needs that identity's own password account.

## Managing enrollment tokens

Requires an already-enrolled admin device's cert/key and the server's CA
cert — enroll your admin device first, with the bootstrap token (above; see
`microtak-server`'s own `docs/ARCHITECTURE.md` "Enrollment security
hardening" section).

```sh
export MICROTAK_ADMIN_SERVER=https://microtak.example.com:8443
export MICROTAK_ADMIN_CERT=admin.pem
export MICROTAK_ADMIN_KEY=admin.key
export MICROTAK_ADMIN_CA=ca.pem

# Mint a token for one device, optionally expiring and noted, and show the
# enrollment QR code to scan with the TAK client
microtak-admin-cli token mint --cn phone-1 --expires-in-secs 86400 \
  --note "for the pixel" --qr --enrollment-url https://192.168.1.10:8446

# List all tokens and their status (active/used/expired/revoked)
microtak-admin-cli token list

# Revoke one before it's used
microtak-admin-cli token revoke <token>
```

### Enrollment QR codes

`--qr` prints the standard TAK enrollment link as a QR code:

```
tak://com.atakmap.app/enroll?host=<host>&username=<device name>&token=<token>
```

Scanning it in ATAK or OmniTAK enrolls the device over HTTPS and connects
it — nothing to type. `--cn` is required with `--qr`/`--pdf`: the token is
**bound** to that device name (it can only enroll that identity, and the
name is the QR code's `username`). `host` and the enrollment port come from
`--enrollment-url` — the address devices reach the server by, e.g.
`https://tak.example.com` (port 443) behind a reverse proxy. OmniTAK also
reads `enrollmentport=`, `port=` (streaming) and `apiport=` (Marti API); the
code includes them only when they differ from 8446/8089/8443 (set the latter
two with `--streaming-port`/`--api-port`). IPv6 literals aren't supported in
the link (TAK clients split host and port on the last `:`) — use a DNS name
or an IPv4 address.

The token is effectively a one-time password for that device name: anyone
who sees the QR code before the device uses it can enroll as that device.
Show or hand it over only to the intended user, prefer short expiries, and
revoke unused ones.

### Printable handouts (QR code + manual enrollment steps)

For handing a token to someone who isn't at a terminal, mint one with
`--pdf` to also write a one-page PDF with the QR code (printed ~6 cm wide,
readable by a phone camera) and type-it-by-hand steps for the TAK client
(host, ports, username, token as password) for when scanning isn't an
option:

```sh
microtak-admin-cli token mint --cn phone-1 --note "for the pixel" \
  --enrollment-url https://192.168.1.10:8446 --pdf handout.pdf
```

To regenerate a handout for a token you already have (e.g. from `token
list`), without minting a new one or contacting the server at all:

```sh
microtak-admin-cli token pdf <token> --cn phone-1 \
  --enrollment-url https://192.168.1.10:8446 --out handout.pdf
```

`token pdf` doesn't check the server, so it has no way to know the token's
real status or expiry — the handout shows "unknown" for expiry in that
case rather than guessing. Fonts (Liberation Sans, SIL Open Font License —
see `assets/fonts/LICENSE-OFL.txt`) are embedded in the binary at compile
time, so PDF generation needs no font files on the machine running it.

Every subcommand's connection flags (`--server`/`--cert`/`--key`/`--ca`)
can be set via the environment variables above instead of repeating them
on every invocation.

If your server's certificate SAN hostname doesn't resolve via normal DNS
yet (e.g. connecting directly to an IP in a lab/grid-down setting before
DNS exists), use `--resolve HOST:PORT:ADDRESS` (the same syntax `curl
--resolve` uses) to override resolution for just that connection.

## Managing password accounts

Some clients (e.g. [CloudTAK](https://github.com/dfpc-coe/CloudTAK)) expect
real username/password login rather than a bare invite token — mint a
password account for them instead:

```sh
# Omit --password to have the server generate one, printed once
microtak-admin-cli user mint cloudtak-alice

microtak-admin-cli user list
microtak-admin-cli user revoke cloudtak-alice
```

That account can then log in via the server's `POST /oauth/token` and
self-enroll a device certificate using `Authorization: Basic` on the
enrollment endpoint — see `microtak-server`'s own
`docs/ARCHITECTURE.md` for the full wire contract. A revoked account can no
longer do either, even with the correct password.

## Managing mission roles

```sh
microtak-admin-cli mission list
microtak-admin-cli mission get "Recon Alpha"
microtak-admin-cli mission set-role "Recon Alpha" some-device subscriber
microtak-admin-cli mission revoke-role "Recon Alpha" some-device
```

You must already hold the `Owner` role on a mission yourself to manage its
roles. Revoking or demoting a mission's last remaining `Owner` is rejected
by the server (a mission can never end up with zero owners) and surfaces
as a normal error with a non-zero exit code.

## License

AGPL-3.0-or-later — see [LICENSE](LICENSE). Same license and reasoning as
`microtak-server` itself: this tool holds a powerful admin credential, and
the same "no wild for-profit fork running this closed" concern applies.
