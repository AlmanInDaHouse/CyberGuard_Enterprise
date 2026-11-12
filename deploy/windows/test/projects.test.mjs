// SPEC-024 Amendment 2026-11-01, point 1: the WiX projects accept only the EULA Manuel accepted,
// each restores against a committed lock file in locked mode, and a package a project references
// is one pins.json pins at its exact version. The lock file covers PackageReference items alone;
// the MSBuild SDK, from global.json, is verified by pins/verify-pins.mjs before any build.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(ROOT, p), "utf8");
const pins = JSON.parse(read("pins/pins.json"));

/** The .wixproj files under deploy/windows, relative to it. */
function projects() {
  const out = [];
  const walk = (dir) => {
    for (const entry of readdirSync(join(ROOT, dir), { withFileTypes: true })) {
      if (entry.isDirectory() && !["bin", "obj", "node_modules"].includes(entry.name)) walk(join(dir, entry.name));
      else if (entry.isFile() && entry.name.endsWith(".wixproj")) out.push(join(dir, entry.name));
    }
  };
  walk(".");
  return out;
}

test("the WiX projects' properties accept the pinned EULA and lock their restore", () => {
  const props = read("Directory.Build.props");
  const group = /<PropertyGroup Condition=" '\$\(MSBuildProjectExtension\)' == '\.wixproj' ">([\s\S]*?)<\/PropertyGroup>/.exec(props);
  assert.ok(group, "Directory.Build.props sets its properties for .wixproj projects alone");
  const value = (name) => new RegExp(`<${name}>([^<]*)</${name}>`).exec(group[1])?.[1];
  assert.equal(value("AcceptEula"), pins.eulaId);
  assert.equal(value("RestorePackagesWithLockFile"), "true");
  assert.equal(value("RestoreLockedMode"), "true");
  assert.equal(value("TreatWarningsAsErrors"), "true");
  assert.equal(value("ProductVersion"), "$(CG_RELEASE_VERSION)");
  assert.equal(props.match(/<PropertyGroup\b/g).length, 1, "no property outside that group");
});

test("each WiX project has a committed lock file and references only pinned packages", () => {
  const found = projects();
  assert.ok(found.length > 0, "a WiX project exists");
  const tracked = new Set(
    execFileSync("git", ["ls-files", "--", "."], { cwd: ROOT, encoding: "utf8" })
      .split("\n")
      .filter(Boolean)
      .map((p) => p.replaceAll("\\", "/")),
  );
  const pinned = new Map(pins.packages.map((p) => [p.id, p.version]));
  for (const project of found) {
    const lock = relative(ROOT, join(ROOT, dirname(project), "packages.lock.json")).replaceAll("\\", "/");
    assert.ok(tracked.has(lock), `${lock} is committed`);
    const text = read(project);
    const references = [...text.matchAll(/<PackageReference\s+Include="([^"]+)"\s+Version="([^"]+)"\s*\/>/g)];
    assert.equal(
      references.length,
      text.match(/<PackageReference\b/g)?.length ?? 0,
      `${project}: each PackageReference has the form this check reads`,
    );
    const locked = JSON.parse(read(lock)).dependencies;
    for (const [, id, version] of references) {
      assert.ok(pinned.has(id), `${project} references ${id}, which pins.json does not pin`);
      assert.equal(version, `[${pinned.get(id)}]`, `${project} takes ${id} at its exact pin`);
    }
    for (const framework of Object.values(locked)) {
      for (const [id, entry] of Object.entries(framework)) {
        assert.equal(entry.resolved, pinned.get(id), `${lock} resolves ${id} at its pin`);
      }
    }
  }
});

test("the MSI's targets check the version and the agent before WiX compiles", () => {
  const targets = read("Directory.Build.targets");
  assert.match(targets, /<Target Name="CheckProductVersion"\s+BeforeTargets="CoreCompile"\s+Condition=" '\$\(MSBuildProjectExtension\)' == '\.wixproj' ">/);
  assert.match(targets, /\.github\\scripts\\release-version\.mjs/);
  assert.match(targets, /Exists\('\$\(AgentExe\)'\)/);
});

// SPEC-024 Amendment 2026-11-12 (second), point 9 and *Tests*: the MSI upgrades a second build of
// its own version, with ICE61 alone suppressed; its one custom action is the test build's, behind
// the CgInjectFailure define, so that a release build has none.
test("the MSI upgrades its own version and carries a custom action only in its test build", () => {
  const wxs = read("msi/Package.wxs");
  const upgrade = wxs.match(/<MajorUpgrade\b[^>]*\/>/g);
  assert.equal(upgrade?.length, 1, "one MajorUpgrade");
  assert.match(upgrade[0], /\bSchedule="afterInstallInitialize"/);
  assert.match(upgrade[0], /\bAllowSameVersionUpgrades="yes"/);
  const guarded = /<\?if \$\(var\.CgInjectFailure\) = "1" \?>([\s\S]*?)<\?endif\?>/g;
  const blocks = [...wxs.matchAll(guarded)];
  assert.equal(blocks.length, 1, "one block behind the test define");
  assert.match(blocks[0][1], /<CustomAction Id="CgInjectedFailure"[^>]*\bExecute="deferred"/);
  const outside = wxs.replace(guarded, "");
  assert.doesNotMatch(outside, /<CustomAction\b|<Custom\b|<\?if\b|<\?ifdef\b|<\?define\b|<\?include\b/,
    "no custom action, condition or definition outside that block");
  const project = read("msi/CyberGuardAgent.wixproj");
  assert.match(project, /<DefineConstants>[^<]*;CgInjectFailure=\$\(CgInjectFailure\)<\/DefineConstants>/);
  assert.deepEqual(
    [...project.matchAll(/<SuppressIces>([^<]*)<\/SuppressIces>/g)].map((m) => m[1]),
    ["ICE61"],
    "ICE61 alone is suppressed",
  );
  for (const file of [...projects(), "Directory.Build.props", "Directory.Build.targets"]) {
    assert.doesNotMatch(read(file), /SuppressAllWarnings|SuppressSpecificWarnings|SuppressValidation|<NoWarn>/,
      `${file} suppresses no other warning or validation`);
  }
});

/** The attributes of the first element `<tag ...>` of `text` that matches `where`, as a map. */
function element(text, tag, where = () => true) {
  for (const [whole] of text.matchAll(new RegExp(`<${tag}\\b[^>]*>`, "g"))) {
    const attributes = new Map([...whole.matchAll(/([\w:]+)="([^"]*)"/g)].map((m) => [m[1], m[2]]));
    if (where(attributes)) return attributes;
  }
  assert.fail(`no <${tag}> as expected`);
}

// SPEC-024 Amendment 2026-11-09, items 1 and 2, and Amendment 2026-11-12 (second), points 1 and
// 8: the setup's chain as built, its variables, and the standard bootstrapper application with
// the project's theme.
test("the setup's chain is preflight, the MSI with the chain's property, a vital boundary and configure", () => {
  const wxs = read("bundle/Bundle.wxs");
  const bundle = element(wxs, "Bundle");
  assert.equal(bundle.get("Version"), "$(var.ProductVersion)", "its version is the release version");
  assert.equal(bundle.get("UpgradeCode"), "B3D1FE6C-5F20-4E58-9B7A-3A7C8E1D4F10", "one upgrade family");
  assert.equal(bundle.get("Manufacturer"), "CyberGuard");
  assert.equal(bundle.get("Copyright"), "Copyright (c) CyberGuard");

  const chain = /<Chain>([\s\S]*?)<\/Chain>/.exec(wxs)?.[1];
  assert.ok(chain, "a chain");
  const order = [...chain.matchAll(/<(ExePackage|MsiPackage|RollbackBoundary)\b[^>]*\bId="([^"]+)"/g)].map((m) => `${m[1]}:${m[2]}`);
  assert.deepEqual(order, ["ExePackage:Preflight", "MsiPackage:Agent", "RollbackBoundary:Committed", "ExePackage:Configure"]);

  const preflight = element(chain, "ExePackage", (a) => a.get("Id") === "Preflight");
  assert.equal(preflight.get("PerMachine"), "yes");
  assert.equal(preflight.get("Vital"), "yes");
  assert.equal(preflight.get("DetectCondition"), "1 = 0", "detected absent: it runs on every install, repair and upgrade");
  assert.equal(preflight.has("Permanent"), false, "not permanent: Burn rolls it back");
  assert.equal(preflight.get("UninstallArguments"), "reverse", "its rollback is the reversal");
  assert.equal(preflight.get("SourceFile"), "$(var.SetupExe)");
  assert.equal(
    preflight.get("InstallArguments"),
    "preflight --bootstrap=&quot;[CG_BOOTSTRAP]&quot; --migrate-legacy=&quot;[CG_MIGRATE_LEGACY]&quot; --trust-anchors=&quot;[CG_TRUST_ANCHORS]&quot;",
  );

  const msi = element(chain, "MsiPackage");
  assert.equal(msi.get("SourceFile"), "$(var.Msi)");
  assert.equal(msi.get("Vital"), "yes");
  const properties = [...chain.matchAll(/<MsiProperty\b[^>]*>/g)].map((m) => m[0]);
  assert.deepEqual(properties, ['<MsiProperty Name="CG_SETUP_CHAIN" Value="1" />'], "the chain's property, alone");

  assert.equal(element(chain, "RollbackBoundary").get("Vital"), "yes");

  const configure = element(chain, "ExePackage", (a) => a.get("Id") === "Configure");
  assert.equal(configure.get("PerMachine"), "yes");
  assert.equal(configure.get("Vital"), "yes");
  assert.equal(configure.get("Permanent"), "yes", "nothing rolls it back or uninstalls it");
  assert.equal(configure.get("DetectCondition"), "", "detected by no condition: it runs on every install, repair and upgrade");
  assert.equal(configure.has("UninstallArguments") || configure.has("RepairArguments"), false);
  assert.equal(configure.get("SourceFile"), "$(var.SetupExe)");
  assert.equal(
    configure.get("InstallArguments"),
    "configure --migrate-legacy=&quot;[CG_MIGRATE_LEGACY]&quot; --trust-anchors=&quot;[CG_TRUST_ANCHORS]&quot;",
  );
});

test("the setup's variables are the unattended route's three, overridable and not persisted", () => {
  const wxs = read("bundle/Bundle.wxs");
  const variables = [...wxs.matchAll(/<Variable\b[^>]*>/g)].map((m) => m[0]);
  assert.deepEqual(
    variables.map((v) => /Name="([^"]+)"/.exec(v)[1]),
    ["CG_BOOTSTRAP", "CG_MIGRATE_LEGACY", "CG_TRUST_ANCHORS"],
  );
  for (const variable of variables) {
    assert.match(variable, /\bType="string"/);
    assert.match(variable, /\bValue=""/);
    assert.match(variable, /\bbal:Overridable="yes"/);
    assert.doesNotMatch(variable, /\bPersisted=|\bHidden=/, "not persisted");
  }
  assert.doesNotMatch(
    wxs,
    /<bal:Condition\b|<\w+:\w*Search\b|<\w*Search\b|<SetVariable\b|<Payload\b|<PayloadGroup|<BundleCustomData|<bal:BootstrapperApplicationPrereq|LaunchTarget/,
    "no condition, search, extra payload or launch of its own: preflight decides",
  );
});

test("the setup's window is the standard application with the project's theme", () => {
  const wxs = read("bundle/Bundle.wxs");
  const application = element(wxs, "bal:WixStandardBootstrapperApplication");
  assert.equal(application.get("Theme"), "hyperlinkLicense");
  assert.equal(application.get("LicenseUrl"), "", "no licence to accept");
  assert.equal(application.has("LicenseFile") || application.has("LogoFile") || application.has("LogoSideFile"), false);
  assert.equal(application.get("ThemeFile"), "$(sys.SOURCEFILEDIR)theme\\CyberGuardTheme.xml");
  assert.equal(application.get("LocalizationFile"), "$(sys.SOURCEFILEDIR)theme\\CyberGuardTheme.wxl");
  assert.equal(application.get("SuppressOptionsUI"), "yes");

  const theme = read("bundle/theme/CyberGuardTheme.xml");
  assert.doesNotMatch(theme, /<ImageControl\b|<Image\b|ImageFile=/, "no image");
  assert.doesNotMatch(theme, /EulaHyperlink|EulaAcceptCheckbox|EulaRichedit/, "no licence");
  const progress = /<Page Name="Progress">([\s\S]*?)<\/Page>/.exec(theme)?.[1];
  assert.ok(progress, "a progress page");
  assert.doesNotMatch(progress, /<Button\b/, "no control while the chain applies");
  const failure = /<Page Name="Failure">([\s\S]*?)<\/Page>/.exec(theme)?.[1];
  assert.ok(failure, "a failure page");
  assert.match(failure, /<Hypertext Name="FailureLogFileLink"/, "it names Burn's log");
  assert.doesNotMatch(failure, /Name="FailureMessageText"/, "not the system text of the code");
  const texts = read("bundle/theme/CyberGuardTheme.wxl");
  assert.match(texts, /<String Id="FailureHyperlinkLogText" Value="[^"]*\[WixBundleLog\]/, "the log's path");
  for (const id of new Set([...theme.matchAll(/#\(loc\.(\w+)\)/g)].map((m) => m[1]))) {
    assert.match(texts, new RegExp(`<String Id="${id}" `), `${id} has its text`);
  }
});

test("the setup's targets check its packages and their versions before WiX compiles", () => {
  const targets = read("Directory.Build.targets");
  assert.match(targets, /'\$\(OutputType\)' == 'Bundle' and \('\$\(Msi\)' == '' or !Exists\('\$\(Msi\)'\)\)/);
  assert.match(targets, /'\$\(OutputType\)' == 'Bundle' and \('\$\(SetupExe\)' == '' or !Exists\('\$\(SetupExe\)'\)\)/);
  assert.match(targets, /'\$\(CheckedMsiVersion\)' != '\$\(CG_RELEASE_VERSION\)'/);
  assert.match(targets, /'\$\(CheckedSetupExeVersion\)' != '\$\(CG_RELEASE_VERSION\)'/);
  const project = read("bundle/CyberGuardSetup.wixproj");
  assert.match(project, /<OutputType>Bundle<\/OutputType>/);
  assert.match(project, /<OutputName>cyberguard-agent-\$\(ProductVersion\)-x64-setup<\/OutputName>/);
  assert.match(project, /<DefineConstants>ProductVersion=\$\(ProductVersion\);Msi=\$\(Msi\);SetupExe=\$\(SetupExe\)<\/DefineConstants>/);
});
