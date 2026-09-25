#Requires -Version 5.1
<#
.SYNOPSIS
Opt-in isolated desktop verification package; never changes certificate trust.
.DESCRIPTION
Register copies a previously built example and registers only LedAlert.Verification.
Launch opens its foreground consent GUI. Report returns 0 only for a recorded PASS.
Toast/RemoveToast create/remove one fixed synthetic fixture for manual GUI cases.
Cleanup unregisters only that identity at this tool's owned path, then removes that path.
No action builds code, enables Developer Mode, requests elevation, or contacts WLED.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('Register', 'Launch', 'Toast', 'RemoveToast', 'Report', 'Cleanup')]
    [string]$Action,
    [string]$Executable,
    [switch]$PrivateDiagnostics
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ([string]::IsNullOrWhiteSpace($Executable)) {
    $Executable = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) '..\target\debug\examples\windows_probe.exe'
}
$identity = 'LedAlert.Verification'
$publisher = 'CN=LedAlertVerification'
$root = Join-Path $env:LOCALAPPDATA 'LedAlertVerification'
$marker = Join-Path $root 'owned-by-windows-verify'
$markerText = 'LedAlert isolated verification schema 1'
$packageRoot = Join-Path $root 'package'
$manifest = Join-Path $packageRoot 'AppxManifest.xml'
$report = Join-Path $root 'report.json'

function Assert-OwnedPath {
    if (!(Test-Path -LiteralPath $marker -PathType Leaf) -or
        (Get-Content -LiteralPath $marker -Raw).Trim() -ne $markerText) {
        throw 'Verification directory is not owned by this helper; refusing to modify it.'
    }
    foreach ($path in @($root, $packageRoot)) {
        if (Test-Path -LiteralPath $path) {
            $item = Get-Item -LiteralPath $path -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw 'Verification path is a reparse point; refusing to follow it.'
            }
        }
    }
}

function Get-OwnedPackage {
    $packages = @(Get-AppxPackage -Name $identity)
    foreach ($package in $packages) {
        if ($package.Publisher -ne $publisher -or
            ![string]::Equals($package.InstallLocation.TrimEnd('\'), $packageRoot.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Verification identity already exists at an unowned location; refusing to modify it.'
        }
    }
    return $packages
}

function Assert-Stopped {
    # Refuse removal/replacement while the copied executable is still in use.
    foreach ($process in @(Get-Process -Name windows_probe -ErrorAction SilentlyContinue)) {
        if ([string]::Equals($process.Path, (Join-Path $packageRoot 'windows_probe.exe'), [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Close the verification GUI before registering, relaunching or cleaning up.'
        }
    }
}

function Invoke-ManualToast([bool]$Remove) {
    Assert-OwnedPath
    $packages = @(Get-OwnedPackage)
    if ($packages.Count -ne 1) { throw 'Register the isolated verification package first.' }
    if ($PSVersionTable.PSEdition -ne 'Desktop') {
        throw 'Toast actions require Windows PowerShell 5.1 (powershell.exe), not pwsh.'
    }
    $appId = "$($packages[0].PackageFamilyName)!Probe"
    [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
    if ($Remove) {
        [Windows.UI.Notifications.ToastNotificationManager]::History.Remove('fixture', 'manual', $appId)
        Write-Output 'REQUESTED: removal of only the manual synthetic fixture.'
    } else {
        [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null
        [Windows.UI.Notifications.ToastNotification, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
        $xml = New-Object Windows.Data.Xml.Dom.XmlDocument
        $xml.LoadXml('<toast><visual><binding template="ToastGeneric"><text>LedAlert verification</text><text>Synthetic fixture; no personal content.</text></binding></visual></toast>')
        $toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
        $toast.Tag = 'fixture'
        $toast.Group = 'manual'
        [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($appId).Show($toast)
        Write-Output 'REQUESTED: synthetic toast. Confirm actual delivery in Windows notification center.'
    }
}

try {
    switch ($Action) {
        'Register' {
            $developerMode = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock' -ErrorAction SilentlyContinue
            if (!$developerMode -or !$developerMode.PSObject.Properties['AllowDevelopmentWithoutDevLicense'] -or $developerMode.AllowDevelopmentWithoutDevLicense -ne 1) {
                throw 'Developer Mode must already be enabled by an authorized user. This helper never enables it or trusts a certificate.'
            }
            if (!(Test-Path -LiteralPath $Executable -PathType Leaf)) {
                throw 'Build first: cargo build --locked --example windows_probe'
            }
            if (Test-Path -LiteralPath $root) {
                Assert-OwnedPath
            } else {
                # Do not acquire ownership over an existing registration, even if its path matches.
                if (@(Get-AppxPackage -Name $identity).Count -ne 0) {
                    throw 'Verification identity already exists; refusing to acquire ownership.'
                }
                New-Item -ItemType Directory -Path $root | Out-Null
                Set-Content -LiteralPath $marker -Value $markerText -Encoding UTF8
            }
            $packages = @(Get-OwnedPackage)
            Assert-Stopped
            if ($packages.Count -ne 0) {
                throw 'Already registered. Use Launch, or Cleanup before rebuilding/registering.'
            }
            New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null
            Copy-Item -LiteralPath $Executable -Destination (Join-Path $packageRoot 'windows_probe.exe')
            # Generate non-personal, correctly sized package logos without external downloads.
            Add-Type -AssemblyName System.Drawing
            foreach ($size in @(44, 150, 50)) {
                $bitmap = New-Object System.Drawing.Bitmap($size, $size)
                $graphics = [Drawing.Graphics]::FromImage($bitmap)
                try {
                    $graphics.Clear([Drawing.Color]::FromArgb(32, 96, 160))
                    $bitmap.Save((Join-Path $packageRoot "logo$size.png"), [Drawing.Imaging.ImageFormat]::Png)
                } finally {
                    $graphics.Dispose()
                    $bitmap.Dispose()
                }
            }
            @'
<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
 xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
 xmlns:uap3="http://schemas.microsoft.com/appx/manifest/uap/windows10/3"
 xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
 IgnorableNamespaces="uap uap3 rescap">
 <Identity Name="LedAlert.Verification" Publisher="CN=LedAlertVerification" Version="1.0.0.0" ProcessorArchitecture="x64" />
 <Properties><DisplayName>LedAlert Verification</DisplayName><PublisherDisplayName>LedAlert verification fixture</PublisherDisplayName><Logo>logo50.png</Logo></Properties>
 <Dependencies><TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.22000.0" MaxVersionTested="10.0.26100.0" /></Dependencies>
 <Resources><Resource Language="en-us" /></Resources>
 <Applications><Application Id="Probe" Executable="windows_probe.exe" EntryPoint="Windows.FullTrustApplication">
  <uap:VisualElements DisplayName="LedAlert Verification" Description="Isolated synthetic listener verification; no WLED" BackgroundColor="transparent" Square150x150Logo="logo150.png" Square44x44Logo="logo44.png" />
 </Application></Applications>
 <Capabilities><uap3:Capability Name="userNotificationListener" /><rescap:Capability Name="runFullTrust" /><rescap:Capability Name="globalMediaControl" /></Capabilities>
</Package>
'@ | Set-Content -LiteralPath $manifest -Encoding UTF8
            Add-AppxPackage -Register $manifest
            if (@(Get-OwnedPackage).Count -ne 1) {
                throw 'Registration did not produce exactly one owned verification package.'
            }
            Write-Output 'REGISTERED: isolated LedAlert.Verification only; no certificate trust changed.'
        }
        'Launch' {
            Assert-OwnedPath
            Assert-Stopped
            $packages = @(Get-OwnedPackage)
            if ($packages.Count -ne 1) { throw 'Register the isolated verification package first.' }
            if (Test-Path -LiteralPath $report) { Remove-Item -LiteralPath $report }
            Start-Process explorer.exe -ArgumentList "shell:AppsFolder\$($packages[0].PackageFamilyName)!Probe"
            Write-Output 'LAUNCHED: click Request access, then Run synthetic lifecycle. Run Report after the final result.'
        }
        'Toast' { Invoke-ManualToast $false }
        'RemoveToast' { Invoke-ManualToast $true }
        'Report' {
            Assert-OwnedPath
            if (!(Test-Path -LiteralPath $report -PathType Leaf)) {
                throw 'No completed report; finish the GUI scenario (or record an aborted run as not verified).'
            }
            $result = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
            $result | ConvertTo-Json -Depth 8
            if ($result.schema -ne 1 -or $result.result -ne 'PASS') {
                throw 'Synthetic lifecycle did not pass.'
            }
        }
        'Cleanup' {
            if (!(Test-Path -LiteralPath $root)) {
                if (@(Get-AppxPackage -Name $identity).Count -ne 0) {
                    throw 'Package exists without ownership directory; refusing to remove it.'
                }
                Write-Output 'CLEAN: no owned verification state exists.'
                break
            }
            Assert-OwnedPath
            Assert-Stopped
            foreach ($package in @(Get-OwnedPackage)) {
                Remove-AppxPackage -Package $package.PackageFullName
            }
            if (@(Get-AppxPackage -Name $identity).Count -ne 0) {
                throw 'Registration remains; preserving files for recovery.'
            }
            # Registration adds hidden package metadata inside this validated owned tree.
            Remove-Item -LiteralPath $root -Recurse -Force
            Write-Output 'CLEAN: removed only owned verification registration and files.'
        }
    }
} catch {
    # Deployment exceptions can contain local paths; disclose them only by explicit opt-in.
    if ($PrivateDiagnostics) {
        [Console]::Error.WriteLine($_.ToString())
    } else {
        [Console]::Error.WriteLine('FAIL: verification action failed. See prerequisites/cleanup in docs/windows-verification.md; use -PrivateDiagnostics only for a private local diagnosis.')
    }
    exit 1
}
