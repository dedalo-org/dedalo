{
  description = "Dedalo — turn code merges into sustainable open-source funding";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, rust-overlay }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      # Every output is built from the same pinned toolchain, so a Nix build,
      # a CI run and a contributor's shell all use one compiler.
      forEachSystem =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ (import rust-overlay) ];
            }
          )
        );

      toolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

      rustPlatformFor =
        pkgs:
        let
          toolchain = toolchainFor pkgs;
        in
        pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };

      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

      # Source filtered down to what actually affects the build, so editing
      # the README does not invalidate a cached compile.
      sourceFor =
        pkgs:
        pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./rust-toolchain.toml
            # rustfmt.toml must be in scope or the fmt check uses defaults.
            ./rustfmt.toml
            ./crates
          ];
        };

      # The workflow files, for the linters that check them. Kept separate
      # from the build source so editing a workflow does not invalidate a
      # cached compile, and vice versa.
      workflowsFor =
        pkgs:
        pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [
            ./.github
            ./action.yml
          ];
        };

      commonArgs = pkgs: {
        pname = "dedalo";
        version = cargoToml.workspace.package.version;
        src = sourceFor pkgs;
        cargoLock.lockFile = ./Cargo.lock;
      };
    in
    {
      packages = forEachSystem (
        pkgs:
        let
          dedalo = (rustPlatformFor pkgs).buildRustPackage (
            (commonArgs pkgs)
            // {
              # The end-to-end tests drive a real repository.
              nativeCheckInputs = [ pkgs.git ];
              preCheck = ''
                export HOME=$TMPDIR
                git config --global user.email "nix@build"
                git config --global user.name "Nix Build"
                git config --global init.defaultBranch main
              '';
              meta = {
                description = "Turn code merges into sustainable open-source funding";
                homepage = cargoToml.workspace.package.repository;
                license = pkgs.lib.licenses.mit;
                mainProgram = "dedalo";
              };
            }
          );
        in
        {
          inherit dedalo;
          default = dedalo;

          # `nix build .#docs` renders the API reference the docs workflow ships.
          docs = (rustPlatformFor pkgs).buildRustPackage (
            (commonArgs pkgs)
            // {
              pname = "dedalo-docs";
              buildPhase = ''
                runHook preBuild
                cargo doc --no-deps --workspace --all-features
                runHook postBuild
              '';
              doCheck = false;
              installPhase = ''
                runHook preInstall
                mkdir -p $out
                cp -r target/doc/* $out/
                echo '<meta http-equiv="refresh" content="0; url=dedalo_core/index.html">' > $out/index.html
                runHook postInstall
              '';
            }
          );
        }
      );

      devShells = forEachSystem (pkgs: {
        default = pkgs.mkShell {
          packages = [
            (toolchainFor pkgs)
            pkgs.cargo-nextest
            pkgs.cargo-deny
            pkgs.cargo-audit
            pkgs.cargo-outdated
            pkgs.cargo-watch
            pkgs.git
            pkgs.taplo
            pkgs.nixfmt-rfc-style
            pkgs.actionlint
            pkgs.zizmor
            pkgs.cargo-semver-checks
            pkgs.shellcheck
          ];

          env.RUST_BACKTRACE = "1";

          shellHook = ''
            echo "dedalo dev shell — rust $(rustc --version | cut -d' ' -f2)"
            echo "  cargo nextest run     run the test suite"
            echo "  cargo clippy --all-targets"
            echo "  cargo doc --open      browse the API reference"
            echo "  actionlint .github/workflows/*.yml"
            echo "  zizmor .github/workflows""
          '';
        };
      });

      checks = forEachSystem (
        pkgs:
        let
          toolchain = toolchainFor pkgs;
        in
        {
          # Building the package runs the full test suite.
          build = self.packages.${pkgs.system}.dedalo;

          clippy = (rustPlatformFor pkgs).buildRustPackage (
            (commonArgs pkgs)
            // {
              pname = "dedalo-clippy";
              nativeBuildInputs = [ toolchain ];
              buildPhase = ''
                runHook preBuild
                cargo clippy --all-targets --all-features -- -D warnings
                runHook postBuild
              '';
              doCheck = false;
              installPhase = "touch $out";
            }
          );

          # The declared MSRV is verified, not asserted: this builds the
          # workspace with exactly the compiler `rust-version` promises.
          msrv =
            let
              msrvToolchain = pkgs.rust-bin.stable.${cargoToml.workspace.package.rust-version}.minimal;
              msrvPlatform = pkgs.makeRustPlatform {
                cargo = msrvToolchain;
                rustc = msrvToolchain;
              };
            in
            msrvPlatform.buildRustPackage (
              (commonArgs pkgs)
              // {
                pname = "dedalo-msrv";
                buildPhase = ''
                  runHook preBuild
                  cargo check --workspace --all-features --all-targets
                  runHook postBuild
                '';
                doCheck = false;
                installPhase = "touch $out";
              }
            );

          # Workflows are code that holds release secrets. actionlint catches
          # what will not run; zizmor catches what will run but should not —
          # script injection, over-broad permissions, unpinned actions.
          actionlint =
            pkgs.runCommand "dedalo-actionlint"
              {
                nativeBuildInputs = [
                  pkgs.actionlint
                  pkgs.shellcheck
                ];
                src = workflowsFor pkgs;
              }
              ''
                cd $src
                actionlint -color .github/workflows/*.yml
                touch $out
              '';

          zizmor =
            pkgs.runCommand "dedalo-zizmor"
              {
                nativeBuildInputs = [ pkgs.zizmor ];
                src = workflowsFor pkgs;
              }
              ''
                cd $src
                # No network in the sandbox, and none needed: every finding
                # here is derivable from the workflow text itself.
                zizmor --offline --no-progress --persona=regular \
                  --min-severity=low .github/workflows action.yml
                touch $out
              '';

          shell =
            pkgs.runCommand "dedalo-shellcheck"
              {
                nativeBuildInputs = [ pkgs.shellcheck ];
                src = pkgs.lib.fileset.toSource {
                  root = ./.;
                  fileset = pkgs.lib.fileset.unions [
                    ./install.sh
                    ./scripts
                  ];
                };
              }
              ''
                cd $src
                shellcheck install.sh scripts/*.sh
                touch $out
              '';

          # The site is hand-written HTML with an inline stylesheet, which
          # means an editing slip ships silently unless something checks.
          site =
            pkgs.runCommand "dedalo-site-check"
              {
                nativeBuildInputs = [ pkgs.python3 ];
                src = pkgs.lib.fileset.toSource {
                  root = ./.;
                  fileset = pkgs.lib.fileset.unions [
                    ./site
                    ./scripts/check-site.py
                  ];
                };
              }
              ''
                cd $src
                python3 scripts/check-site.py --root site
                touch $out
              '';

          # rustfmt needs no dependencies, so this check is nearly free.
          fmt =
            pkgs.runCommand "dedalo-fmt-check"
              {
                nativeBuildInputs = [ toolchain ];
                src = sourceFor pkgs;
              }
              ''
                cd $src
                cargo fmt --all --check
                touch $out
              '';
        }
      );

      apps = forEachSystem (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.system}.dedalo}/bin/dedalo";
        };
      });

      overlays.default = final: _prev: {
        dedalo = self.packages.${final.system}.dedalo;
      };

      formatter = forEachSystem (pkgs: pkgs.nixfmt-rfc-style);
    };
}
