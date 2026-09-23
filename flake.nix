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

          src = craneLib.cleanCargoSource ./.;

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
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          microtak-admin-cli = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              pname = "microtak-admin-cli";
              cargoExtraArgs = "--locked --bin microtak-admin-cli";
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

            test = craneLib.cargoTest (commonArgs // { inherit cargoArtifacts; });

            clippy = craneLib.cargoClippy (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--all-targets -- -D warnings";
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
