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
release's (install_ac_015), which is not built yet.
`test/msi-version-matrix.mjs` checks that its build refuses a version outside the release's form.
