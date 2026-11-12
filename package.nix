{ lib, stdenv, rustPlatform, python3, git, rustfmt, clippy }:
rustPlatform.buildRustPackage {
  pname = "syslog-exporter";
  version = "0.14.0";
  src = lib.cleanSource ./.;
  cargoLock.lockFile = ./Cargo.lock;
  cargoBuildType = "release";
  CARGO_BUILD_TARGET = stdenv.hostPlatform.rust.rustcTarget;
  nativeCheckInputs = [ python3 git rustfmt clippy ];
  doCheck = true;
  checkPhase = ''
    runHook preCheck
    export CARGO_BUILD_JOBS=$NIX_BUILD_CORES RUST_TEST_THREADS=$NIX_BUILD_CORES
    bash scripts/check --release
    runHook postCheck
  '';
  meta.platforms = lib.platforms.linux;
}
