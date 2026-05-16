$ErrorActionPreference = "Stop"

$Repo = "itzmail/relay"
$BinName = "relay.exe"
$InstallDir = "$env:USERPROFILE\.relay\bin"
$Asset = "relay-windows-x86_64.zip"

# Get latest release tag
$Latest = (Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest").tag_name

if (-not $Latest) {
    Write-Error "Failed to fetch latest release"
    exit 1
}

Write-Host "Installing relay $Latest..."

$Tmp = New-TemporaryFile | ForEach-Object { $_.DirectoryName + "\" + $_.BaseName }
New-Item -ItemType Directory -Path $Tmp | Out-Null

$Url = "https://github.com/$Repo/releases/download/$Latest/$Asset"
Invoke-WebRequest -Uri $Url -OutFile "$Tmp\$Asset"
Expand-Archive -Path "$Tmp\$Asset" -DestinationPath $Tmp

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item "$Tmp\$BinName" "$InstallDir\$BinName" -Force
Remove-Item -Recurse -Force $Tmp

# Add to PATH if not already there
$CurrentPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($CurrentPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$CurrentPath;$InstallDir", "User")
    Write-Host "Added $InstallDir to PATH (restart terminal to apply)"
}

Write-Host "relay $Latest installed to $InstallDir\$BinName"
Write-Host "Run 'relay setup claude-code --global' to get started."
