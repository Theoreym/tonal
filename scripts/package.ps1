param([switch]$SkipBuild, [string]$InnoCompiler = '')
$ErrorActionPreference = 'Stop'
$tonalRoot = Split-Path $PSScriptRoot -Parent
Set-Location $tonalRoot
$tonalVersion = [regex]::Match([IO.File]::ReadAllText((Join-Path $tonalRoot 'Cargo.toml')), '(?m)^version = "(\d+\.\d+\.\d+)"').Groups[1].Value
if (-not $tonalVersion) { throw 'Invalid Tonal version.' }
if (-not $SkipBuild) {
    $env:PATH = [Environment]::GetEnvironmentVariable('Path','Machine')+';'+[Environment]::GetEnvironmentVariable('Path','User')
    $tonalCmake = Join-Path $tonalRoot 'target\build-tools\cmake-wheel\cmake\data\bin\cmake.exe'
    if (-not $env:CMAKE -and (Test-Path $tonalCmake)) { $env:CMAKE = $tonalCmake }
    & (Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe') build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
}
if (-not $InnoCompiler) { $InnoCompiler = Join-Path $tonalRoot 'target\build-tools\inno-setup\compiler\ISCC.exe' }
if (-not (Test-Path -LiteralPath $InnoCompiler)) { throw 'Install Inno Setup or pass -InnoCompiler with the ISCC.exe path.' }
$tonalDist = Join-Path $tonalRoot 'dist'
$tonalStageRoot = Join-Path $tonalRoot 'target\package-staging'
$tonalWork = Join-Path $tonalStageRoot ([Guid]::NewGuid().ToString('N'))
$tonalName = "tonal-$tonalVersion-windows-x64"
$tonalPackage = Join-Path $tonalWork $tonalName
$tonalBin = Join-Path $tonalPackage 'bin'
$tonalLicenses = Join-Path $tonalPackage 'licenses'
New-Item -ItemType Directory -Force $tonalDist,$tonalBin,$tonalLicenses | Out-Null
try {
    Copy-Item -LiteralPath (Join-Path $tonalRoot 'target\release\tonal.exe') -Destination $tonalPackage
    Copy-Item -LiteralPath (Join-Path $tonalRoot 'LICENSE') -Destination (Join-Path $tonalLicenses 'Tonal-MIT.txt')
    Copy-Item -LiteralPath (Join-Path $tonalRoot 'assets\licenses\libopus.txt') -Destination (Join-Path $tonalLicenses 'libopus.txt')
    Copy-Item -LiteralPath (Join-Path $tonalRoot 'docs\QUICKSTART.md') -Destination (Join-Path $tonalPackage 'LIRE-MOI.md')
    Copy-Item -Path (Join-Path $tonalRoot 'assets\ejs\LICENSE*'),(Join-Path $tonalRoot 'assets\ejs\DEPENDENCIES.txt') -Destination $tonalLicenses
    $tonalNode = (Get-Command node -ErrorAction Stop).Source
    $tonalYtdlp = (Get-Command yt-dlp -ErrorAction Stop).Source
    $tonalNodeVersion = (& $tonalNode --version).Trim()
    $tonalYtdlpVersion = (& $tonalYtdlp --version).Trim()
    if ($tonalNodeVersion -notmatch '^v\d+\.\d+\.\d+$' -or $tonalYtdlpVersion -notmatch '^\d{4}\.\d{2}\.\d{2}$') { throw 'Unexpected runtime-tool version.' }
    $tonalNodeSums = (Invoke-WebRequest "https://nodejs.org/dist/$tonalNodeVersion/SHASUMS256.txt").Content
    $tonalYtdlpSums = (Invoke-WebRequest "https://github.com/yt-dlp/yt-dlp/releases/download/$tonalYtdlpVersion/SHA2-256SUMS").Content
    if ($tonalNodeSums -is [byte[]]) { $tonalNodeSums = [Text.Encoding]::UTF8.GetString($tonalNodeSums) }
    if ($tonalYtdlpSums -is [byte[]]) { $tonalYtdlpSums = [Text.Encoding]::UTF8.GetString($tonalYtdlpSums) }
    foreach ($tonalTool in @(@($tonalNode,'win-x64/node.exe',$tonalNodeSums,'node.exe'),@($tonalYtdlp,'yt-dlp.exe',$tonalYtdlpSums,'yt-dlp.exe'))) {
        $tonalExpected = [regex]::Match($tonalTool[2], '(?m)^([a-fA-F0-9]{64})\s+\*?'+[regex]::Escape($tonalTool[1])+'\r?$').Groups[1].Value
        $tonalActual = (Get-FileHash -LiteralPath $tonalTool[0] -Algorithm SHA256).Hash
        if (-not $tonalExpected -or $tonalExpected -ne $tonalActual) { throw ('Official checksum mismatch: '+$tonalTool[3]) }
        Copy-Item -LiteralPath $tonalTool[0] -Destination (Join-Path $tonalBin $tonalTool[3])
    }
    Invoke-WebRequest "https://raw.githubusercontent.com/nodejs/node/$tonalNodeVersion/LICENSE" -OutFile (Join-Path $tonalLicenses 'Node-LICENSE.txt')
    Invoke-WebRequest "https://raw.githubusercontent.com/yt-dlp/yt-dlp/$tonalYtdlpVersion/LICENSE" -OutFile (Join-Path $tonalLicenses 'yt-dlp-LICENSE.txt')
    Invoke-WebRequest "https://raw.githubusercontent.com/yt-dlp/yt-dlp/$tonalYtdlpVersion/THIRD_PARTY_LICENSES.txt" -OutFile (Join-Path $tonalLicenses 'yt-dlp-THIRD-PARTY.txt')
    Invoke-WebRequest 'https://www.gnu.org/licenses/gpl-3.0.txt' -OutFile (Join-Path $tonalLicenses 'yt-dlp-binary-GPL-3.0.txt')
    $tonalSources = @("Node.js $tonalNodeVersion : https://github.com/nodejs/node/tree/$tonalNodeVersion", "yt-dlp $tonalYtdlpVersion : https://github.com/yt-dlp/yt-dlp/tree/$tonalYtdlpVersion", 'The official standalone yt-dlp executable contains GPLv3+ components; its license and component notices are included. It runs as a separate process from Tonal.', 'Component source locations are listed in Node-LICENSE.txt and yt-dlp-THIRD-PARTY.txt.')
    $tonalSources | Set-Content (Join-Path $tonalLicenses 'SOURCES.txt') -Encoding utf8
    # App-local CRT: no Visual Studio, SDK or system-wide C++ installation required.
    $tonalVswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $tonalVs = & $tonalVswhere -latest -products '*' -property installationPath
    $tonalCrt = Get-ChildItem -Path (Join-Path $tonalVs 'VC\Redist\MSVC\*\x64\Microsoft.VC143.CRT') -Directory | Sort-Object FullName | Select-Object -Last 1
    if (-not $tonalCrt) { throw 'MSVC redistributable files not found.' }
    foreach ($tonalDll in @('vcruntime140.dll','vcruntime140_1.dll')) {
        $tonalSource = Join-Path $tonalCrt.FullName $tonalDll
        $tonalSignature = Get-AuthenticodeSignature $tonalSource
        if ($tonalSignature.Status -ne 'Valid' -or $tonalSignature.SignerCertificate.Subject -notmatch 'Microsoft Corporation') { throw 'Unexpected CRT signature.' }
        Copy-Item -LiteralPath $tonalSource -Destination $tonalPackage
    }
    $tonalWebview = Join-Path $tonalRoot 'target\package-tools\MicrosoftEdgeWebview2Setup.exe'
    New-Item -ItemType Directory -Force (Split-Path $tonalWebview -Parent) | Out-Null
    if (-not (Test-Path $tonalWebview)) { Invoke-WebRequest 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $tonalWebview }
    $tonalWebviewSignature = Get-AuthenticodeSignature $tonalWebview
    if ($tonalWebviewSignature.Status -ne 'Valid' -or $tonalWebviewSignature.SignerCertificate.Subject -notmatch 'Microsoft Corporation') { throw 'Unexpected WebView2 signature.' }
    Copy-Item -LiteralPath $tonalWebview -Destination $tonalBin
    @('Microsoft Visual C++ runtime: copyright Microsoft Corporation.', 'Redistributed app-local files: vcruntime140.dll, vcruntime140_1.dll.', 'Redistributable list: https://aka.ms/vs/17/redist.txt', 'License terms: https://visualstudio.microsoft.com/license-terms/vs2022-cruntime/', 'Microsoft WebView2 bootstrapper: https://developer.microsoft.com/microsoft-edge/webview2/', 'Distribution: https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution') | Set-Content (Join-Path $tonalLicenses 'Microsoft-components.txt') -Encoding utf8
    $tonalManifest = [pscustomobject]@{Version=$tonalVersion;Architecture='x64';Signed=(Get-AuthenticodeSignature (Join-Path $tonalPackage 'tonal.exe')).Status.ToString();Node=$tonalNodeVersion;Ytdlp=$tonalYtdlpVersion;Files=@(Get-ChildItem $tonalPackage -Recurse -File | ForEach-Object {[pscustomobject]@{Path=$_.FullName.Substring($tonalPackage.Length+1);Bytes=$_.Length;Sha256=(Get-FileHash $_.FullName -Algorithm SHA256).Hash}})}
    $tonalManifest | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $tonalPackage 'manifest.json') -Encoding utf8
    $tonalZip = Join-Path $tonalDist ($tonalName+'.zip')
    if (Test-Path -LiteralPath $tonalZip) { Remove-Item -LiteralPath $tonalZip }
    Compress-Archive -LiteralPath $tonalPackage -DestinationPath $tonalZip -CompressionLevel Optimal
    & $InnoCompiler "/DAppVersion=$tonalVersion" "/DPackageDir=$tonalPackage" "/DOutputPath=$tonalDist" (Join-Path $tonalRoot 'installer\tonal.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
    $tonalArtifacts = @(Get-ChildItem -LiteralPath $tonalDist -File | Where-Object Name -like ($tonalName+'*') | ForEach-Object {[pscustomobject]@{File=$_.Name;MiB=[math]::Round($_.Length/1MB,2);Sha256=(Get-FileHash $_.FullName -Algorithm SHA256).Hash}})
    $tonalArtifacts | ForEach-Object { $_.Sha256.ToLowerInvariant()+'  '+$_.File } | Set-Content (Join-Path $tonalDist 'SHA256SUMS.txt') -Encoding ascii
    $tonalArtifacts | ConvertTo-Json -Compress
} finally {
    # Delete only this run's generated staging folder; never the project, tools or dist.
    $tonalChecked = [IO.Path]::GetFullPath($tonalWork)
    $tonalAllowed = [IO.Path]::GetFullPath($tonalStageRoot)+[IO.Path]::DirectorySeparatorChar
    if ($tonalChecked.StartsWith($tonalAllowed,[StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $tonalChecked)) { Remove-Item -LiteralPath $tonalChecked -Recurse -Force }
}
