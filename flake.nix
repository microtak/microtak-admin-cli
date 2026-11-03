{
  description = "microtak-admin-cli: command-line admin tool for a MicroTAK server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    { self, nixpkgs, flake-utils, rust-overlay, crane }:
    flake-utils.lib.eachSystem
      [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ]
      (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          rustToolchain = pkgs.rust-bin.stable.latest.default;
          craneLib = (crane.mkLib pkgs).overrideToolchain (_: rustToolchain);

          # cleanCargoSource keeps only Rust/Cargo files; the handout PDF's fonts
          # (assets/fonts/*.ttf) are include_bytes!'d into the binary, so keep
          # assets/ too.
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            name = "source";
            filter =
              path: type:
              (craneLib.filterCargoSources path type) || (builtins.match ".*/assets(/.*)?$" path != null);
          };

          commonArgs = {
            inherit src;
            strictDeps = true;
            # rustls-tls (not openssl-sys) is used throughout, so no system
            # TLS library is needed here -- just the usual Darwin frameworks
            # reqwest/rcgen pull in for networking and certificate handling.
            buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
              pkgs.libiconv
              pkgs.darwin.apple_sdk.frameworks.Security
              pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
            ];
            # Cargo.toml has a [dev-dependencies] path dependency on a sibling
            # ../microtak-server checkout (only tests/cli.rs uses it). Even a
            # plain `cargo build` resolves the whole manifest, so the path has
            # to exist, at the version Cargo.lock records -- a stub crate does
            # it without pulling the real server into the build. The stub has
            # none of the real crate's dependencies, so cargo drops those from
            # the lock: build --offline rather than --locked (every crate is
            # still the one Cargo.lock pins, vendored from it; nothing is fetched).
            cargoExtraArgs = "--offline";
            postUnpack = ''
              mkdir -p "$NIX_BUILD_TOP/microtak-server/src"
              cat > "$NIX_BUILD_TOP/microtak-server/Cargo.toml" <<'EOF'
              [package]
              name = "microtak-server"
              version = "0.10.0"
              edition = "2021"
              [lib]
              path = "src/lib.rs"
              EOF
              : > "$NIX_BUILD_TOP/microtak-server/src/lib.rs"
            '';
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          microtak-admin-cli = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              pname = "microtak-admin-cli";
              cargoExtraArgs = "--offline --bin microtak-admin-cli";
              doCheck = false; # see `checks.test` below -- run once, not twice
            }
          );
        in
        {
          packages = {
            default = microtak-admin-cli;
            microtak-admin-cli = microtak-admin-cli;
          };

          apps.default = flake-utils.lib.mkApp {
            drv = microtak-admin-cli;
            name = "microtak-admin-cli";
          };

          checks = {
            inherit microtak-admin-cli;

            # Lib and bin only: the tests and --all-targets need the real
            # server crate (see postUnpack above), so tests/cli.rs stays in CI.
            clippy = craneLib.cargoClippy (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--bins -- -D warnings";
              }
            );
          };

          devShells.default = pkgs.mkShell {
            inputsFrom = [ microtak-admin-cli ];
            packages = [ rustToolchain ];
          };
        }
      );
}
