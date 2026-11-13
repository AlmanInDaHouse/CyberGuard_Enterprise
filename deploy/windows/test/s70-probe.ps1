# PROBE ONLY (Session 70, never lands): runs the setup on the runner quietly, passively, with
# its full window to the end, and with its full window closed while preflight holds, and prints
# what Burn's log and the instrumented cg-setup recorded.
param([string]$Setup, [string]$Out)
$ErrorActionPreference = 'Continue'
New-Item -ItemType Directory -Force $Out, 'C:\cg-probe' | Out-Null

Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices; using System.Collections.Generic;
public static class U {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc f, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  public static string Text(IntPtr h){var s=new StringBuilder(512);GetWindowText(h,s,512);return s.ToString();}
  public static string Cls(IntPtr h){var s=new StringBuilder(256);GetClassName(h,s,256);return s.ToString();}
  public static uint Pid(IntPtr h){uint p;GetWindowThreadProcessId(h,out p);return p;}
  public static List<IntPtr> Top(){var l=new List<IntPtr>();EnumWindows((h,x)=>{l.Add(h);return true;},IntPtr.Zero);return l;}
  public static List<IntPtr> Kids(IntPtr p){var l=new List<IntPtr>();EnumChildWindows(p,(h,x)=>{l.Add(h);return true;},IntPtr.Zero);return l;}
}
"@

function Show-Windows($label) {
  $fg = [U]::GetForegroundWindow()
  Write-Host "--- windows ($label); foreground $fg '$([U]::Text($fg))' pid $([U]::Pid($fg))"
  foreach ($h in [U]::Top()) {
    if ([U]::IsWindowVisible($h)) {
      $t = [U]::Text($h)
      if ($t) { Write-Host "  $h pid=$([U]::Pid($h)) class=$([U]::Cls($h)) text='$t'" }
    }
  }
}

function Find-SetupWindow {
  foreach ($h in [U]::Top()) {
    if ([U]::IsWindowVisible($h) -and [U]::Text($h) -eq 'CyberGuard Agent Setup') { return $h }
  }
  return [IntPtr]::Zero
}

function Wait-SetupWindow($seconds) {
  $deadline = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $deadline) {
    $h = Find-SetupWindow
    if ($h -ne [IntPtr]::Zero) { return $h }
    Start-Sleep -Milliseconds 500
  }
  return [IntPtr]::Zero
}

function Click-Install($window) {
  foreach ($k in [U]::Kids($window)) {
    $t = [U]::Text($k)
    if ($t -eq '&Install' -or $t -eq 'Install') {
      Write-Host "clicking $k class=$([U]::Cls($k)) visible=$([U]::IsWindowVisible($k))"
      [U]::PostMessage($k, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
      return $true
    }
  }
  Write-Host "no Install button among:"
  foreach ($k in [U]::Kids($window)) { Write-Host "  $k class=$([U]::Cls($k)) visible=$([U]::IsWindowVisible($k)) text='$([U]::Text($k))'" }
  return $false
}

function Wait-Exit($p, $seconds) {
  if (-not $p.WaitForExit($seconds * 1000)) { Write-Host "setup still running after $seconds s"; return $null }
  return $p.ExitCode
}

function Show-Burn($log) {
  Write-Host "--- Burn log $log"
  if (Test-Path $log) {
    Select-String -Path $log -Pattern 'preflight|Preflight|rollback|Rollback|abandon|Exit code|Apply complete|cancel|Cancel|UILevel' |
      ForEach-Object { Write-Host "  $($_.Line)" }
  } else { Write-Host "  (absent)" }
}

$me = [System.Diagnostics.Process]::GetCurrentProcess()
Write-Host "runner: session $($me.SessionId); user $(whoami); interactive $([Environment]::UserInteractive)"
whoami /groups | Select-String 'Mandatory Label' | ForEach-Object { Write-Host "  $($_.Line.Trim())" }
query user 2>&1 | ForEach-Object { Write-Host "  query user: $_" }
Show-Windows 'start'

# A: quiet.
Set-Content C:\cg-probe\hold.txt '5'
$p = Start-Process $Setup -ArgumentList '-quiet','-norestart','-log',"$Out\A-quiet.log" -PassThru
Write-Host "=== A quiet exit $(Wait-Exit $p 300)"
Show-Burn "$Out\A-quiet.log"

# P: passive.
$p = Start-Process $Setup -ArgumentList '-passive','-norestart','-log',"$Out\P-passive.log" -PassThru
Write-Host "=== P passive exit $(Wait-Exit $p 300)"
Show-Burn "$Out\P-passive.log"

# C: full window, preflight holds 20 s then refuses 22; the failure page is then closed.
Set-Content C:\cg-probe\hold.txt '20'
$p = Start-Process $Setup -ArgumentList '-norestart','-log',"$Out\C-full.log" -PassThru
$w = Wait-SetupWindow 90
Write-Host "=== C window $w"
Show-Windows 'C before install'
if ($w -ne [IntPtr]::Zero -and (Click-Install $w)) {
  Start-Sleep -Seconds 8
  Show-Windows 'C preflight holding'
  Start-Sleep -Seconds 30
  Show-Windows 'C after preflight'
  $w = Find-SetupWindow
  if ($w -ne [IntPtr]::Zero) { [U]::PostMessage($w, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
}
Write-Host "=== C full exit $(Wait-Exit $p 180)"
Show-Burn "$Out\C-full.log"

# B: full window closed while preflight holds 150 s.
Set-Content C:\cg-probe\hold.txt '150'
$p = Start-Process $Setup -ArgumentList '-norestart','-log',"$Out\B-cancel.log" -PassThru
$w = Wait-SetupWindow 90
Write-Host "=== B window $w"
if ($w -ne [IntPtr]::Zero -and (Click-Install $w)) {
  Start-Sleep -Seconds 15
  Show-Windows 'B preflight holding'
  Write-Host "closing the setup's window at $(Get-Date -Format o)"
  [U]::PostMessage($w, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
  Start-Sleep -Seconds 3
  Show-Windows 'B after close'
}
Write-Host "=== B cancel exit $(Wait-Exit $p 180) at $(Get-Date -Format o)"
Show-Burn "$Out\B-cancel.log"
Start-Sleep -Seconds 15
Get-Process cg-setup -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "still running: cg-setup $($_.Id)" }
Get-Process cg-setup -ErrorAction SilentlyContinue | Stop-Process -Force

foreach ($f in Get-ChildItem C:\cg-probe -Filter 'probe-*.log') {
  Write-Host "--- $($f.Name)"
  Get-Content $f.FullName | ForEach-Object { Write-Host "  $_" }
  Copy-Item $f.FullName $Out
}
Write-Host "--- data root"
Get-ChildItem 'C:\ProgramData\CyberGuard' -Recurse -Force -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "  $($_.FullName)" }
exit 0
