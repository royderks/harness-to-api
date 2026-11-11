$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) ('h2a-install-test-' + [Guid]::NewGuid())
$null = New-Item -ItemType Directory $root
$env:H2A_INSTALL_DIR = Join-Path $root 'install with spaces'
$env:H2A_INSTALL_ONLY = '1'
$installer = Join-Path $PSScriptRoot '../../install.ps1'
try {
    # Exercise the same scriptblock execution used by the README, against the public release.
    & ([scriptblock]::Create((Get-Content -Raw $installer)))
    $binary = Join-Path $env:H2A_INSTALL_DIR 'h2a.exe'
    $version = & $binary version
    if ($LASTEXITCODE -ne 0 -or $version -ne 'v0.1.0') { throw 'Installed binary version failed' }
    $original = (Get-FileHash $binary).Hash
    # A repeat install must replace an existing executable successfully.
    & ([scriptblock]::Create((Get-Content -Raw $installer)))
    if ((Get-FileHash $binary).Hash -ne $original) { throw 'Repeat install changed bytes' }
    # Corrupt the checksum at the HTTP boundary; installation must fail before replacement.
    function Invoke-WebRequest {
        param($Uri, $OutFile, [switch]$UseBasicParsing)
        if ($Uri -like '*checksums*') {
            $archive = ($Uri.Split('/')[-1] -replace '^checksums_', 'h2a_' -replace '\.txt$', '.tar.gz')
            Set-Content -LiteralPath $OutFile -Value (('0' * 64) + ' *' + $archive)
        } else {
            Microsoft.PowerShell.Utility\Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $OutFile
        }
    }
    $rejected = $false
    try { & ([scriptblock]::Create((Get-Content -Raw $installer))) }
    catch { if ($_.ToString() -notmatch 'Checksum mismatch') { throw }; $rejected = $true }
    if (-not $rejected -or (Get-FileHash $binary).Hash -ne $original) { throw 'Bad checksum altered installation' }
    Remove-Item Function:\Invoke-WebRequest
    Write-Host 'PASS: native Windows install, version, repeat, checksum refusal'
} finally {
    Remove-Item -LiteralPath $root -Recurse -Force
    Remove-Item Env:\H2A_INSTALL_DIR, Env:\H2A_INSTALL_ONLY -ErrorAction SilentlyContinue
}
