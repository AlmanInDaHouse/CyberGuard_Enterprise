# Spike, never landed: what stock Burn and the MSI do on a hosted runner. Prints facts; never fails.
param([string]$Out123, [string]$Out124)
$ErrorActionPreference = 'Continue'
$logs = Join-Path $env:RUNNER_TEMP 'spike-logs'
New-Item -ItemType Directory -Force $logs | Out-Null
$setup123 = Join-Path $Out123 'cyberguard-agent-1.2.3-x64-setup.exe'
$setup124 = Join-Path $Out124 'cyberguard-agent-1.2.4-x64-setup.exe'

function Run-Setup($name, $exe, [string[]]$extra) {
  $log = Join-Path $logs "$name.log"
  $p = Start-Process -FilePath $exe -ArgumentList (@('-quiet', '-norestart', '-log', "`"$log`"") + $extra) -Wait -PassThru
  Write-Host "### $name exit=$($p.ExitCode)"
  if (Test-Path $log) {
    Select-String -Path $log -Pattern 'Applying execute package|Applied execute package|Detected package|Plan.*package|bal:Condition|condition.*false|Exit code|restart' |
      Select-Object -First 30 | ForEach-Object { Write-Host "    $($_.Line)" }
  }
}
function Show-State($label) {
  Write-Host "--- state: $label"
  sc.exe qc CyberGuardAgent | Select-String 'BINARY_PATH|START_TYPE|SERVICE_START_NAME|FAILED' | ForEach-Object { Write-Host "    $_" }
  sc.exe query CyberGuardAgent | Select-String 'STATE|FAILED' | ForEach-Object { Write-Host "    $_" }
  sc.exe qfailure CyberGuardAgent | Select-String 'RESET|RESTART|FAILURE_ACTIONS|FAILED' | ForEach-Object { Write-Host "    $_" }
  sc.exe sdshow CyberGuardAgent | ForEach-Object { if ($_) { Write-Host "    $_" } }
  $exe = 'C:\Program Files\CyberGuard\Agent\cg-agent.exe'
  Write-Host "    exe exists: $(Test-Path $exe)"
  if (Test-Path $exe) { Write-Host "    exe version: $(& $exe --version)" ; Write-Host "    exe acl: $((Get-Acl $exe).Sddl)" ; Write-Host "    dir acl: $((Get-Acl (Split-Path $exe)).Sddl)" }
  Write-Host "    ProgramData\CyberGuard exists: $(Test-Path 'C:\ProgramData\CyberGuard')"
  $k = 'HKLM:\SYSTEM\CurrentControlSet\Services\EventLog\Application\CyberGuardAgent'
  if (Test-Path $k) { Get-ItemProperty $k | Select-Object EventMessageFile, TypesSupported | Format-List | Out-String | Write-Host }
  Get-CimInstance -ClassName Win32_Product -Filter "Name='CyberGuard Agent'" | ForEach-Object { Write-Host "    product: $($_.Version)" }
}

Show-State 'before'
Run-Setup 'install-123' $setup123 @()
Show-State 'after install 1.2.3'
Run-Setup 'rerun-123' $setup123 @()
Show-State 'after rerun 1.2.3'
Run-Setup 'upgrade-124' $setup124 @()
Show-State 'after upgrade 1.2.4'
Run-Setup 'downgrade-123' $setup123 @()
Show-State 'after downgrade attempt'
Run-Setup 'uninstall-124' $setup124 @('-uninstall')
Show-State 'after uninstall'

Run-Setup 'preflight-fails' $setup123 @('CG_SPIKE_PREFLIGHT=--no-such-flag')
Show-State 'after preflight failure'

Run-Setup 'configure-fails' $setup123 @('CG_SPIKE_CONFIGURE=--no-such-flag')
Show-State 'after configure failure'
Run-Setup 'silent-retry' $setup123 @()
Show-State 'after silent retry'
Run-Setup 'uninstall-after-failure' $setup123 @('-uninstall')
Show-State 'after second uninstall'

New-Item -ItemType Directory -Force 'C:\ProgramData\CyberGuard\agent' | Out-Null
Set-Content 'C:\ProgramData\CyberGuard\agent\cert.pem' 'x'
Run-Setup 'console-layout' $setup123 @()
Show-State 'console layout refused'
Remove-Item -Recurse -Force 'C:\ProgramData\CyberGuard'

Write-Host '--- bundle registration leftovers'
Get-ChildItem 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall' -ErrorAction SilentlyContinue |
  Where-Object { (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue).DisplayName -like 'CyberGuard*' } |
  ForEach-Object { Write-Host "    $($_.PSChildName)" }
Get-ChildItem 'C:\ProgramData\Package Cache' -ErrorAction SilentlyContinue | Where-Object { $_.Name -like '*cg-*' -or $_.Name -like '{*' } | Select-Object -First 8 | ForEach-Object { Write-Host "    cache: $($_.Name)" }
exit 0
