$ErrorActionPreference = "Stop"

$BinName = "relay.exe"
$InstallDir = "$env:USERPROFILE\.relay\bin"
$BinPath = "$InstallDir\$BinName"

if (-not (Test-Path $BinPath)) {
    Write-Host "relay not found at $BinPath"
    exit 0
}

Remove-Item -Force $BinPath
Write-Host "relay removed from $BinPath"

# Remove from PATH if present
$CurrentPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($CurrentPath -like "*$InstallDir*") {
    $NewPath = ($CurrentPath -split ";" | Where-Object { $_ -ne $InstallDir }) -join ";"
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
    Write-Host "Removed $InstallDir from PATH"
}
