# Only disposable GitHub-hosted Windows runners may install these fixtures.
# No application executable is started. Neither installer receives /R.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
  throw 'Upgrade smoke tests may run only on disposable GitHub-hosted Windows runners.'
}
$taskRoot = Join-Path $env:RUNNER_TEMP ('orbitpal-upgrade-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $taskRoot | Out-Null
$config = Get-Content 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json
$expectedVersion = $config.version
if ($config.identifier -ne 'studio.streamer-ai.cohost' -or $config.productName -ne 'OrbitPal') { throw 'Unexpected app migration identity.' }
$bundleRoot = Join-Path $PWD 'src-tauri/target/release/bundle'
$newNsis = @(Get-ChildItem (Join-Path $bundleRoot 'nsis') -Filter "OrbitPal_${expectedVersion}_x64-setup.exe")
$newMsi = @(Get-ChildItem (Join-Path $bundleRoot 'msi') -Filter "OrbitPal_${expectedVersion}_x64*.msi")
if ($newNsis.Count -ne 1 -or $newMsi.Count -ne 1) { throw 'Expected exactly one new NSIS and MSI installer.' }
$uninstallRoots = @(
  'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
  'HKCU:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
  'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
  'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
)
function Get-ProductEntries {
  $records = foreach ($root in $uninstallRoots) {
    if (Test-Path $root) {
      Get-ChildItem $root | ForEach-Object {
        $record = Get-ItemProperty $_.PSPath
        if ($record.PSObject.Properties['DisplayName'] -and $record.DisplayName -in @('Streamer AI Cohost', 'OrbitPal')) { $record }
      }
    }
  }
  @($records)
}
function Invoke-Installer([string]$File, [string[]]$Arguments, [int[]]$Allowed = @(0, 3010)) {
  $process = Start-Process -FilePath $File -ArgumentList $Arguments -Wait -PassThru
  if ($process.ExitCode -notin $Allowed) { throw "Installer failed with exit code $($process.ExitCode): $File" }
}
function Download-Pinned([string]$Name, [string]$Sha256) {
  $destination = Join-Path $taskRoot $Name
  Invoke-WebRequest -Uri ('https://github.com/ralphschuler/streamer-ai-cohost/releases/download/v0.7.0/' + $Name) -OutFile $destination
  if ((Get-FileHash $destination -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Sha256) { throw "Published v0.7.0 fixture checksum mismatch: $Name" }
  $destination
}
function Assert-OnlyVersion([string]$Version, [string]$Name) {
  $entries = @(Get-ProductEntries)
  if ($entries.Count -ne 1 -or $entries[0].DisplayVersion -ne $Version -or $entries[0].DisplayName -ne $Name) {
    $entries | Select-Object PSPath, DisplayName, DisplayVersion | Format-Table | Out-String | Write-Host
    throw "Expected exactly one $Name uninstall registration at version $Version."
  }
  $entries[0]
}
if (@(Get-ProductEntries).Count) { throw 'Runner already has a cohost installation; refusing to modify it.' }
$oldNsis = Download-Pinned 'Streamer-AI-Cohost-v0.7.0-windows-x64-setup.exe' 'ab84318ea972caeb101de1ac7a0997994ca66e7494c0faa0d9385d41ba88dc39'
$oldMsi = Download-Pinned 'Streamer-AI-Cohost-v0.7.0-windows-x64.msi' '16f645c09093460a975b0c98f3e6d12c19a6e6600f4f8e43eda12d8c31c0eec9'
$appData = Join-Path $env:APPDATA 'studio.streamer-ai.cohost'
New-Item -ItemType Directory -Path $appData -Force | Out-Null
$sentinel = Join-Path $appData 'rename-upgrade-fixture.json'
$sentinelValue = '{"fixture":"' + [guid]::NewGuid() + '","kept":true}'
$desktop = [Environment]::GetFolderPath('Desktop')
$startMenu = [Environment]::GetFolderPath('Programs')
New-Item -ItemType Directory -Path $desktop -Force | Out-Null
$shell = New-Object -ComObject WScript.Shell

# NSIS: deliberately omit /D on the new installer so the legacy registry entry
# must restore the old installation directory. /UPDATE mirrors in-app upgrades.
$nsisDirectory = Join-Path $taskRoot 'nsis-installed'
Invoke-Installer $oldNsis @('/S', "/D=$nsisDirectory") @(0)
$oldEntry = Assert-OnlyVersion '0.7.0' 'Streamer AI Cohost'
$oldBinary = Join-Path $nsisDirectory 'streamer-ai-cohost.exe'
if (-not (Test-Path $oldBinary)) { throw 'Legacy NSIS executable missing.' }
$legacyShortcut = Join-Path $desktop 'Streamer AI Cohost.lnk'
$link = $shell.CreateShortcut($legacyShortcut); $link.TargetPath = $oldBinary; $link.Save()
Set-Content $sentinel -Value $sentinelValue -NoNewline
Invoke-Installer $newNsis[0].FullName @('/S', '/UPDATE') @(0)
$newEntry = Assert-OnlyVersion $expectedVersion 'OrbitPal'
$newBinary = Join-Path $nsisDirectory 'orbitpal.exe'
if (-not (Test-Path $newBinary) -or (Test-Path $oldBinary)) { throw 'NSIS did not replace the legacy executable in place.' }
if ($newEntry.PSPath -ne $oldEntry.PSPath) { throw 'NSIS upgrade changed the persistent uninstall identity.' }
if ((Get-Content $sentinel -Raw) -ne $sentinelValue) { throw 'NSIS upgrade changed existing app data.' }
foreach ($shortcut in @($legacyShortcut, (Join-Path $desktop 'OrbitPal.lnk'))) {
  if (-not (Test-Path $shortcut) -or $shell.CreateShortcut($shortcut).TargetPath -ne $newBinary) { throw "NSIS shortcut was not migrated: $shortcut" }
}
Write-Host 'NSIS v0.7.0 → OrbitPal upgrade preserved one install, executable target, shortcuts and app data.'
# The uninstaller runs with /S only: no checkbox requesting data deletion.
Invoke-Installer (Join-Path $nsisDirectory 'uninstall.exe') @('/S', "_?=$nsisDirectory") @(0)
if (@(Get-ProductEntries).Count) { throw 'NSIS cleanup left an installed product.' }
if ((Get-Content $sentinel -Raw) -ne $sentinelValue) { throw 'NSIS cleanup unexpectedly removed app data.' }

# MSI: the pinned former UpgradeCode must remove the old product. The new
# display/install directory may change; the bundle identifier and data do not.
$msiDirectory = Join-Path $taskRoot 'msi-installed'
Invoke-Installer 'msiexec.exe' @('/i', "`"$oldMsi`"", '/qn', '/norestart', "INSTALLDIR=`"$msiDirectory`"", '/L*v', "`"$(Join-Path $taskRoot 'old-msi.log')`"")
$oldEntry = Assert-OnlyVersion '0.7.0' 'Streamer AI Cohost'
Set-Content $sentinel -Value $sentinelValue -NoNewline
Invoke-Installer 'msiexec.exe' @('/i', "`"$($newMsi[0].FullName)`"", '/qn', '/norestart', '/L*v', "`"$(Join-Path $taskRoot 'new-msi.log')`"")
$newEntry = Assert-OnlyVersion $expectedVersion 'OrbitPal'
if ($newEntry.PSChildName -eq $oldEntry.PSChildName) { throw 'MSI upgrade retained the old version product code.' }
$installCandidates = @($msiDirectory, (Join-Path $env:ProgramFiles 'OrbitPal'))
if ($newEntry.PSObject.Properties['InstallLocation'] -and $newEntry.InstallLocation) { $installCandidates += $newEntry.InstallLocation.Trim('"') }
$newBinaries = @($installCandidates | Select-Object -Unique | ForEach-Object { Join-Path $_ 'orbitpal.exe' } | Where-Object { Test-Path $_ })
if ($newBinaries.Count -ne 1) { throw 'MSI upgrade did not produce exactly one OrbitPal executable.' }
if (Test-Path (Join-Path $msiDirectory 'streamer-ai-cohost.exe')) { throw 'MSI upgrade left the old executable behind.' }
if ((Get-Content $sentinel -Raw) -ne $sentinelValue) { throw 'MSI upgrade changed existing app data.' }
$shortcuts = @((Join-Path $startMenu 'OrbitPal.lnk'), (Join-Path $startMenu 'OrbitPal\OrbitPal.lnk'), (Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\OrbitPal.lnk'), (Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\OrbitPal\OrbitPal.lnk'))
if (-not @($shortcuts | Where-Object { (Test-Path $_) -and $shell.CreateShortcut($_).TargetPath -eq $newBinaries[0] }).Count) { throw 'MSI did not create a working OrbitPal Start menu shortcut.' }
Write-Host 'MSI v0.7.0 → OrbitPal upgrade preserved one product, new executable/shortcut and app data.'
Invoke-Installer 'msiexec.exe' @('/x', $newEntry.PSChildName, '/qn', '/norestart', '/L*v', "`"$(Join-Path $taskRoot 'remove-msi.log')`"")
if (@(Get-ProductEntries).Count) { throw 'MSI cleanup left an installed product.' }
if ((Get-Content $sentinel -Raw) -ne $sentinelValue) { throw 'MSI cleanup unexpectedly removed app data.' }
Remove-Item $sentinel
Write-Host 'Windows installer migration smoke completed; no application process was started.'
