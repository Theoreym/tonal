$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
# Include tools installed since the current terminal/editor was opened.
$tonalPathEntries = (@($env:Path,
    [Environment]::GetEnvironmentVariable('Path', 'Machine'),
    [Environment]::GetEnvironmentVariable('Path', 'User')) -join ';').Split(';') |
    Where-Object { $_ } | Select-Object -Unique
$env:Path = $tonalPathEntries -join ';'
$tonalCmake=Join-Path $PSScriptRoot '..\target\build-tools\cmake-wheel\cmake\data\bin\cmake.exe'
if (-not $env:CMAKE -and (Test-Path -LiteralPath $tonalCmake)) { $env:CMAKE=(Resolve-Path -LiteralPath $tonalCmake).Path }
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) {
    $rustupCargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
    if (-not (Test-Path -LiteralPath $rustupCargo)) {
        throw 'Rust est absent. Installe Rust via https://rustup.rs/ puis relance ce script.'
    }
    & $rustupCargo run --release --locked
} else {
    & $cargo.Source run --release --locked
}
exit $LASTEXITCODE
