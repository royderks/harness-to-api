param([string]$Version, [string]$Platform)
$ErrorActionPreference = 'Stop'
$dist = (Resolve-Path dist).Path
$root = Join-Path ([IO.Path]::GetTempPath()) ('h2a-candidate-' + [Guid]::NewGuid())
$null = New-Item -ItemType Directory $root
$env:H2A_INSTALL_DIR = Join-Path $root 'install with spaces'
$env:H2A_INSTALL_ONLY = '1'
$installer = Get-Content -Raw (Join-Path $dist 'candidate-installers/install.ps1')
$script:badChecksum = $false
$script:failDownload = $false
function Invoke-WebRequest {
    param($Uri, $OutFile, [switch]$UseBasicParsing)
    if ($script:failDownload) { throw 'Simulated download failure' }
    if ($Uri -notlike "*/download/$Version/*") { throw 'Unexpected candidate URL' }
    $name = $Uri.Split('/')[-1]
    if ($script:badChecksum -and $name -like 'checksums*') {
        Set-Content -LiteralPath $OutFile -Value (('0' * 64) + "  h2a_${Version}_${Platform}.tar.gz")
    } else { Copy-Item -LiteralPath (Join-Path $dist $name) -Destination $OutFile }
}
try {
    $binary = Join-Path $env:H2A_INSTALL_DIR 'h2a.exe'
    for ($i = 0; $i -lt 2; $i++) { & ([scriptblock]::Create($installer)) }
    if ((& $binary version) -ne $Version -or $LASTEXITCODE -ne 0) { throw 'Installed version mismatch' }
    $original = (Get-FileHash $binary -Algorithm SHA256).Hash
    $extract = Join-Path $root 'extract'
    $null = New-Item -ItemType Directory $extract
    & tar.exe -xzf (Join-Path $dist "h2a_${Version}_${Platform}.tar.gz") -C $extract ./h2a.exe
    if ($LASTEXITCODE -ne 0 -or (Get-FileHash (Join-Path $extract 'h2a.exe') -Algorithm SHA256).Hash -ne $original) { throw 'Installed bytes differ from candidate archive' }
    foreach ($failure in @('checksum', 'download')) {
        $script:badChecksum = $failure -eq 'checksum'
        $script:failDownload = $failure -eq 'download'
        $rejected = $false
        try { & ([scriptblock]::Create($installer)) } catch { $rejected = $true }
        if (-not $rejected -or (Get-FileHash $binary -Algorithm SHA256).Hash -ne $original) { throw "$failure did not preserve installation" }
    }
    Write-Host 'PASS: candidate Windows installer, version, repeat, checksum and download refusal; interactive chooser not tested'
} finally {
    Remove-Item Function:\Invoke-WebRequest -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $root -Recurse -Force
    Remove-Item Env:\H2A_INSTALL_DIR, Env:\H2A_INSTALL_ONLY -ErrorAction SilentlyContinue
}
