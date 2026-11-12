# Probe only, never lands: applies one mutant of the MSI's source, so that the MSI's tests can be
# seen red against an MSI missing exactly one behaviour (CLAUDE.md compensating control 1).
param([Parameter(Mandatory)][string]$Mutant)
$ErrorActionPreference = 'Stop'
$wxs = Join-Path $PSScriptRoot '..\msi\Package.wxs'
$text = Get-Content -Raw $wxs
function Replace-Once([string]$from, [string]$to) {
  if (-not $script:text.Contains($from)) { throw "mutant ${Mutant}: '$from' not found" }
  $script:text = $script:text.Replace($from, $to)
}
switch ($Mutant) {
  'none' { }
  # M1: no setup-chain condition.
  'no-chain' { Replace-Once 'Condition="Installed OR CG_SETUP_CHAIN = &quot;1&quot;"' 'Condition="1"' }
  # M2: no per-machine condition.
  'no-per-machine' { Replace-Once 'Condition="ALLUSERS = 1 AND NOT MSIINSTALLPERUSER"' 'Condition="1"' }
  # M3: Windows' default service DACL.
  'default-dacl' { $script:text = [regex]::Replace($script:text, '\s*<PermissionEx Sddl="[^"]*" />', '') }
  # M4: no Application log source.
  'no-event-source' { $script:text = [regex]::Replace($script:text, '(?s)\s*<RegistryKey Root="HKLM".*?</RegistryKey>', '') }
  # M5: the MSI creates %ProgramData%\CyberGuard.
  'creates-data' {
    Replace-Once '    <Feature Id="Agent">' @'
    <StandardDirectory Id="CommonAppDataFolder">
      <Directory Id="ProbeData" Name="CyberGuard">
        <Component Id="ProbeData" Guid="5C3B4E9A-1D2F-4A6B-8C7D-9E0F1A2B3C4D">
          <CreateFolder />
        </Component>
      </Directory>
    </StandardDirectory>
    <Feature Id="Agent">
      <ComponentRef Id="ProbeData" />
'@
  }
  # M6: the configuration path unquoted.
  'unquoted-config' { Replace-Once '--config &quot;[CommonAppDataFolder]CyberGuard\agent\agent.toml&quot;' '--config [CommonAppDataFolder]CyberGuard\agent\agent.toml' }
  # M7: a manual start.
  'demand-start' { Replace-Once 'Start="auto"' 'Start="demand"' }
  # M8: Users may modify the binary's directory.
  'loose-directory' {
    Replace-Once '            <File Id="cg_agent_exe"' @'
            <CreateFolder>
              <PermissionEx Sddl="D:(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;BU)" />
            </CreateFolder>
            <File Id="cg_agent_exe"
'@
  }
  # M9: the service is not removed on uninstall.
  'service-kept' { Replace-Once 'Stop="uninstall" Remove="uninstall"' 'Stop="uninstall"' }
  default { throw "unknown mutant $Mutant" }
}
Set-Content -NoNewline -Encoding utf8 $wxs $text
Write-Host "PROBE mutant $Mutant applied"
