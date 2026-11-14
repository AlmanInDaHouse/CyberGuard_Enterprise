# Windows package

The sources of the agent's MSI and setup (SPEC-024 §Operational §5), built with WiX v7 under its
Amendment 2026-11-01, point 1. The pins come first: `global.json` (the .NET SDK and WiX's MSBuild
SDK), `.config/dotnet-tools.json` (the `wix` tool), `pins/pins.json` (each package's SHA-512) and
`pins/Pins.csproj`, which restores them so that `pins/verify-pins.mjs` can hash them before use.

```sh
cd deploy/windows
dotnet restore pins/Pins.csproj
node pins/verify-pins.mjs
dotnet tool restore
node pins/verify-pins.mjs --tool
```

`dotnet` reads `global.json` and the tool manifest upwards from the working directory, so these
commands run from this directory. `.github/workflows/package-ci.yml` runs them on Windows.

## The MSI

`msi/` is the agent's MSI (SPEC-024 §Data contracts §1 to §3, its Amendments 2026-11-09, item 4,
and 2026-11-12). `Directory.Build.props` gives the WiX projects the accepted EULA, a locked restore
against each project's committed `packages.lock.json`, and their release version, which
`Directory.Build.targets` checks with the release's own check before WiX compiles. After the pins
are verified, with the agent's release executable built at the same version:

```sh
export CG_RELEASE_VERSION=1.2.3
cargo build --release -p cg-agent
cd deploy/windows
dotnet build --no-incremental msi/CyberGuardAgent.wixproj -c Release -p:AgentExe=<repo>/target/release/cg-agent.exe
```

The MSI is written to `msi/bin/Release/cyberguard-agent-<version>-x64.msi`. Building it installs
nothing. Its launch conditions admit an installation only with the setup chain's property
`CG_SETUP_CHAIN=1`, or on an installed product, and only per machine; `package-ci`'s `msi` job
installs it on a hosted runner with that property and sees a direct installation, a direct major
upgrade and a per-user installation refused (`agent/cg-setup/tests/package_msi_host.rs`). It
upgrades a second build of its own version as it upgrades a lower one (SPEC-024 Amendment
2026-11-12 (second), point 9), with ICE61, the validation that warns of it, suppressed alone.
`-p:CgInjectFailure=1` makes a test build whose deferred action fails inside the transaction;
a release build has no custom action, which the job reads. The
build does not compare the agent's own version with `CG_RELEASE_VERSION`: that comparison is the
release's (install_ac_015), below.
`test/msi-version-matrix.mjs` checks that its build refuses a version outside the release's form.

## The setup

`bundle/` is the agent's setup, `cyberguard-agent-<version>-x64-setup.exe` (SPEC-024 Amendments
2026-11-09, items 1 and 2, and 2026-11-12 (second) and (third)): WiX's Burn engine and standard
bootstrapper application with the project's theme (`bundle/theme/`), whose chain runs `cg-setup`
preflight, the MSI with `CG_SETUP_CHAIN=1`, a vital rollback boundary and `cg-setup` configure.
Preflight is told the setup's window, `--ui="[WixBundleUILevel]"`: only with the full window, on a
fresh or pending host given no bootstrap file or migration variable, does it show the wizard
(SPEC-024 Amendment 2026-11-13, points 1 to 3); with `-quiet` or `-passive` it asks nothing. `Directory.Build.targets` checks, before WiX compiles, that the MSI and `cg-setup.exe` it carries
are of the release version. After the MSI is built, with `cg-setup.exe` built at the same version:

```sh
export CG_RELEASE_VERSION=1.2.3
cargo build --release -p cg-agent -p cg-setup
cd deploy/windows
dotnet build --no-incremental bundle/CyberGuardSetup.wixproj -c Release -p:Msi=<msi> -p:SetupExe=<repo>/target/release/cg-setup.exe
```

The setup is written to `bundle/bin/Release/cyberguard-agent-<version>-x64-setup.exe`. Building it
installs nothing; running it installs the agent, so it runs only in `package-ci`'s `bundle` job, on
a hosted runner (`agent/cg-setup/tests/package_bundle_host.rs`), never on a development host.
`test/bundle-version-matrix.mjs` checks that its build refuses a version outside the release's form
and packages of another version.

## The release

`.github/scripts/release-windows.mjs` makes the release's four Windows assets (SPEC-024 Amendment
2026-11-14): `release.yml`'s agent job runs it at the tag's version, and `package-ci`'s `release`
job at a fixture version, the release's dry run. From the repository root, with the toolchains of
the gate:

```sh
node .github/scripts/check-licences.mjs --report <dir>/licences-1.2.3.md
node .github/scripts/release-windows.mjs build --version 1.2.3 --out <empty dir> --work <empty dir> --report <dir>/licences-1.2.3.md
```

It refuses to start unless the output and work directories are empty, the report carries WiX's
`LICENSE.TXT` and `git status` shows no change or untracked file under `deploy/windows`, `agent`,
`Cargo.toml`, `Cargo.lock`, `.cargo` or `rust-toolchain.toml`; relative directories are taken from
the current one. Its steps run without `RUSTFLAGS`, the compiler wrappers, a cargo target or target
directory, `CgInjectFailure`, MSBuild's `Custom*MicrosoftCommonTargets` or `NUGET_PACKAGES` from the
environment. It checks the pins and the EULA ID, builds the agent alone and then `cg-setup` at that
version, the MSI with the test build's define empty and the setup, and then checks them without
installing anything: the
executable's version resource, imports and message table, the MSI's version, its one file and no
custom action, the setup's version and copyright, and, with the pinned `wix` tool, that the MSI the
setup carries and the agent's file the MSI carries are byte-identical to the assets. Last it builds
`wix-<release>-source.tar.gz`, WiX's tree at the commit and tree IDs `pins/pins.json` records,
fetched from WiX's repository, and the setup's own sources under `cyberguard-setup/deploy/windows/`,
and reads it back against the pinned tree. `test/release-check-matrix.mjs` sees each check refuse the
wrong set it is for, and `test/release-workflow.test.mjs` holds `release.yml`'s structure.
