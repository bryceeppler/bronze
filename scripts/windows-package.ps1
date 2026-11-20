param([string]$TargetDir = "target\package-build")
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"$' | Select-Object -First 1).Matches[0].Groups[1].Value
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME ".cargo" }
$env:CARGO_ENCODED_RUSTFLAGS = "--remap-path-prefix=$cargoHome=/cargo" + [char]0x1f + "--remap-path-prefix=$((Get-Location).Path)=/bronze"
cargo build --release --locked --target-dir $TargetDir
if ($LASTEXITCODE) { exit $LASTEXITCODE }

$name = "bronze-$version-windows-x86_64"
$stage = Join-Path $TargetDir "stage"
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force (Join-Path $stage $name), "target\package" | Out-Null
Copy-Item (Join-Path $TargetDir "release\bronze.exe"), LICENSE -Destination (Join-Path $stage $name)
$zip = "target\package\$name.zip"
Remove-Item -Force $zip -ErrorAction SilentlyContinue
& "$env:SystemRoot\System32\tar.exe" -a -c -f $zip -C $stage $name
if ($LASTEXITCODE) { exit $LASTEXITCODE }
Write-Output "windows-package: packaged $zip"
