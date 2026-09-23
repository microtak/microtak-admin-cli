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

## Enrolling a device

Talks to the server's plain (unauthenticated by design) enrollment
endpoint — generates a keypair and CSR, submits it, and saves the signed
certificate and key:

```sh
microtak-admin-cli enroll \
  --enrollment-url http://microtak.example.com:8446 \
  --cn my-device \
  --out-cert my-device.pem --out-key my-device.key
```

Add `--token <token>` if the server is locked down (see below for minting
one) — by default (`enrollment_mode = "auto"`), a fresh server accepts
enrollment with no token until its configured admin device has enrolled,
then requires one for everyone else from that moment on, live, no restart.

## Managing enrollment tokens

Requires an already-enrolled admin device's cert/key and the server's CA
cert — enroll your admin device first, while the server is still open (see
`microtak-server`'s own `docs/ARCHITECTURE.md` "Enrollment lockdown / admin
API" section for how `enrollment_mode = "auto"` locks down the moment that
device exists).

```sh
export MICROTAK_ADMIN_SERVER=https://microtak.example.com:8443
export MICROTAK_ADMIN_CERT=admin.pem
export MICROTAK_ADMIN_KEY=admin.key
export MICROTAK_ADMIN_CA=ca.pem

# Mint a token, optionally expiring, optionally noted, optionally shown as
# a QR code (encoding a microtak-enroll: URI with the enrollment URL and
# token together, so a device operator can scan instead of copy-pasting)
microtak-admin-cli token mint --expires-in-secs 86400 --note "for jz_pixel" \
  --qr --enrollment-url http://microtak.example.com:8446

# List all tokens and their status (active/used/expired/revoked)
microtak-admin-cli token list

# Revoke one before it's used
microtak-admin-cli token revoke <token>
```

Every subcommand's connection flags (`--server`/`--cert`/`--key`/`--ca`)
can be set via the environment variables above instead of repeating them
on every invocation.

If your server's certificate SAN hostname doesn't resolve via normal DNS
yet (e.g. connecting directly to an IP in a lab/grid-down setting before
DNS exists), use `--resolve HOST:PORT:ADDRESS` (the same syntax `curl
--resolve` uses) to override resolution for just that connection.

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
