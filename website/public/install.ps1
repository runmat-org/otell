# otell install script for Windows
# Usage: iwr https://otell.dev/install.ps1 -useb | iex

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repo = "runmat-org/otell"
$apiUrl = "https://api.github.com/repos/$repo/releases/latest"

Write-Host "Fetching latest otell release metadata..."
$release = Invoke-RestMethod -Uri $apiUrl
$tag = $release.tag_name

if ([string]::IsNullOrWhiteSpace($tag)) {
  throw "Failed to resolve latest release tag"
}

$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne "AMD64") {
  throw "Unsupported Windows architecture '$arch'. Use manual install: https://github.com/$repo/releases"
}

$suffix = "windows-x86_64"
$asset = "$tag-$suffix.zip"
$downloadUrl = "https://github.com/$repo/releases/download/$tag/$asset"

$tempDir = Join-Path $env:TEMP ("otell-install-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempDir | Out-Null

try {
  $archivePath = Join-Path $tempDir $asset
  Write-Host "Downloading $asset..."
  Invoke-WebRequest -Uri $downloadUrl -OutFile $archivePath

  Write-Host "Extracting archive..."
  Expand-Archive -Path $archivePath -DestinationPath $tempDir -Force

  $binaryPath = Join-Path $tempDir ("$tag-$suffix\otell.exe")
  if (-not (Test-Path $binaryPath)) {
    throw "Extracted binary not found at $binaryPath"
  }

  $installDir = Join-Path $env:LOCALAPPDATA "Programs\otell"
  New-Item -ItemType Directory -Path $installDir -Force | Out-Null
  Copy-Item -Path $binaryPath -Destination (Join-Path $installDir "otell.exe") -Force

  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if ([string]::IsNullOrEmpty($userPath)) {
    $userPath = ""
  }

  if (-not ($userPath -split ";" | Where-Object { $_ -eq $installDir })) {
    $newPath = if ($userPath.Length -gt 0) { "$userPath;$installDir" } else { $installDir }
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    Write-Host "Added $installDir to your user PATH"
    Write-Host "Open a new terminal session to use otell"
  }

  Write-Host "Installed otell to $installDir\otell.exe"
  Write-Host "Run: otell intro"
}
finally {
  if (Test-Path $tempDir) {
    Remove-Item -Path $tempDir -Recurse -Force
  }
}
