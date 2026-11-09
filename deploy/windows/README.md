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
