# Changelog

## [0.3.0](https://github.com/microtak/microtak-admin-cli/compare/v0.2.0...v0.3.0) (2026-09-29)


### ⚠ BREAKING CHANGES

* **token:** QR/PDF payloads changed format; `token mint --qr/--pdf` and `token pdf` require --cn; `token pdf` takes the link flags (--enrollment-url, --streaming-port, --api-port).
* **enroll:** `enroll` requires an https:// enrollment URL; against a server using its own CA, --ca is required.

### Features

* add user subcommand, fix bare-base64 cert parsing ([cc487a3](https://github.com/microtak/microtak-admin-cli/commit/cc487a3f322995f0a47e2c2919b6fe76224566f2))
* **enroll:** enroll over HTTPS, verifying the server's CA ([#5](https://github.com/microtak/microtak-admin-cli/issues/5)) ([023575b](https://github.com/microtak/microtak-admin-cli/commit/023575b0f5c847c5d9bd366342223d14dbb9eaf1))
* generate printable enrollment handout PDFs (QR code + manual steps) ([0a1516a](https://github.com/microtak/microtak-admin-cli/commit/0a1516a8ab4bb65138f538a8f9569b8ee3e61a90))
* initial microtak-admin-cli ([099d112](https://github.com/microtak/microtak-admin-cli/commit/099d1121313a7e70d4f713485656458b892047dd))
* manage groups (channels); groups on tokens and accounts; read-only mission role ([#7](https://github.com/microtak/microtak-admin-cli/issues/7)) ([fc7842a](https://github.com/microtak/microtak-admin-cli/commit/fc7842a3bb3583f4647f2391a4dd445429142caf))
* **nix:** add flake.nix ([21ec22c](https://github.com/microtak/microtak-admin-cli/commit/21ec22c524ee9c3c47caee2b91a17d5fad23a3d9))
* **token:** standard tak:// enrollment QR codes, tokens bound to a device ([#6](https://github.com/microtak/microtak-admin-cli/issues/6)) ([cd59d36](https://github.com/microtak/microtak-admin-cli/commit/cd59d361257c2a5c7b8109258a8cb7f9471c7ae4))


### Documentation

* enroll the admin device with the server's bootstrap token ([#4](https://github.com/microtak/microtak-admin-cli/issues/4)) ([f5d0fa1](https://github.com/microtak/microtak-admin-cli/commit/f5d0fa126598357cf3ff5d7e8ed576f33901cfa3))
* **readme:** update enrollment lockdown references to Auto/Open ([851516f](https://github.com/microtak/microtak-admin-cli/commit/851516fc828255313e940e1aeab8310734815b0d))

## [0.2.0](https://github.com/microtak/microtak-admin-cli/compare/v0.1.0...v0.2.0) (2026-09-24)


### Features

* add user subcommand, fix bare-base64 cert parsing ([cc487a3](https://github.com/microtak/microtak-admin-cli/commit/cc487a3f322995f0a47e2c2919b6fe76224566f2))
* generate printable enrollment handout PDFs (QR code + manual steps) ([0a1516a](https://github.com/microtak/microtak-admin-cli/commit/0a1516a8ab4bb65138f538a8f9569b8ee3e61a90))
* initial microtak-admin-cli ([099d112](https://github.com/microtak/microtak-admin-cli/commit/099d1121313a7e70d4f713485656458b892047dd))
* **nix:** add flake.nix ([21ec22c](https://github.com/microtak/microtak-admin-cli/commit/21ec22c524ee9c3c47caee2b91a17d5fad23a3d9))


### Documentation

* **readme:** update enrollment lockdown references to Auto/Open ([851516f](https://github.com/microtak/microtak-admin-cli/commit/851516fc828255313e940e1aeab8310734815b0d))
