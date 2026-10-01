param(
    [string]$Destination = (Join-Path $PSScriptRoot "..\src-tauri\resources\winsw")
)

$ErrorActionPreference = "Stop"
$releaseVersion = "v2.12.0"
$releaseUrl = "https://github.com/winsw/winsw/releases/download/$releaseVersion/WinSW.NET461.exe"
$licenseUrl = "https://raw.githubusercontent.com/winsw/winsw/$releaseVersion/LICENSE.txt"
$checksumPath = Join-Path $PSScriptRoot "winsw-sha256.txt"
$checksum = (Get-Content -LiteralPath $checksumPath -Raw).Trim().ToLowerInvariant()
$destinationDirectory = [System.IO.Path]::GetFullPath($Destination)
$executablePath = Join-Path $destinationDirectory "WinSW.NET461.exe"
$licensePath = Join-Path $destinationDirectory "LICENSE.txt"

if ($checksum -notmatch '^[0-9a-f]{64}$') {
    throw "The pinned WinSW checksum is invalid."
}

New-Item -ItemType Directory -Path $destinationDirectory -Force | Out-Null
if (Test-Path -LiteralPath $executablePath) {
    $hashInput = [System.IO.File]::OpenRead($executablePath)
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $actualChecksum = [System.BitConverter]::ToString($hasher.ComputeHash($hashInput)).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $hashInput.Dispose()
        $hasher.Dispose()
    }
}
else {
    Invoke-WebRequest -Uri $releaseUrl -OutFile $executablePath
    $hashInput = [System.IO.File]::OpenRead($executablePath)
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $actualChecksum = [System.BitConverter]::ToString($hasher.ComputeHash($hashInput)).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $hashInput.Dispose()
        $hasher.Dispose()
    }
}
if ($actualChecksum -ne $checksum) {
    Remove-Item -LiteralPath $executablePath -Force -ErrorAction SilentlyContinue
    throw "The local or downloaded WinSW binary did not match its pinned SHA-256 checksum."
}

if (-not (Test-Path -LiteralPath $licensePath)) {
    Invoke-WebRequest -Uri $licenseUrl -OutFile $licensePath
}
Write-Output "Verified WinSW $releaseVersion ($actualChecksum)."
