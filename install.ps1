# Keep the entry point last so a truncated download cannot start installation.
function Install-H2a {
    $ErrorActionPreference = 'Stop'
    $version = 'v0.1.0'
    if ($env:OS -ne 'Windows_NT') { throw 'Use install.sh on macOS or Linux.' }
    $architecture = $env:PROCESSOR_ARCHITEW6432
    if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
    switch ($architecture) {
        'AMD64' { $arch = 'amd64' }
        'ARM64' { $arch = 'arm64' }
        default { throw 'Supported architectures: x86-64 and ARM64.' }
    }
    $null = Get-Command tar.exe -ErrorAction Stop
    if (-not $HOME) { throw 'HOME must be set.' }
    $destination = $env:H2A_INSTALL_DIR
    if (-not $destination) { $destination = Join-Path $HOME '.local\bin' }
    if ($destination -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+)') {
        throw 'Install directory must be an absolute path.'
    }
    $temp = Join-Path ([IO.Path]::GetTempPath()) ('h2a-install-' + [Guid]::NewGuid())
    $staged = $null
    try {
        $null = New-Item -ItemType Directory -Path $temp
        $archive = "h2a_${version}_windows_${arch}.tar.gz"
        $base = "https://github.com/royderks/harness-to-api/releases/download/$version"
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$archive" -OutFile (Join-Path $temp $archive)
        Invoke-WebRequest -UseBasicParsing -Uri "$base/checksums_${version}_windows_${arch}.txt" -OutFile (Join-Path $temp 'checksum')
        $checksum = (Get-Content -Raw -LiteralPath (Join-Path $temp 'checksum')).Trim()
        if ($checksum -notmatch ('^([a-fA-F0-9]{64})\s+\*?' + [regex]::Escape($archive) + '$')) {
            throw 'Invalid release checksum.'
        }
        $expected = $Matches[1]
        if ((Get-FileHash -LiteralPath (Join-Path $temp $archive) -Algorithm SHA256).Hash -ne $expected) {
            throw 'Checksum mismatch; installation stopped.'
        }
        & tar.exe -xzf (Join-Path $temp $archive) -C $temp ./h2a.exe
        if ($LASTEXITCODE -ne 0) { throw 'Archive extraction failed.' }
        $binary = Get-Item -LiteralPath (Join-Path $temp 'h2a.exe')
        if ($binary.PSIsContainer -or ($binary.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'Archive does not contain a regular h2a.exe executable.'
        }
        $null = New-Item -ItemType Directory -Path $destination -Force
        $target = Join-Path $destination 'h2a.exe'
        $staged = Join-Path $destination ('.h2a-install-' + [Guid]::NewGuid())
        Copy-Item -LiteralPath $binary.FullName -Destination $staged
        if (Test-Path -LiteralPath $target) {
            [IO.File]::Replace($staged, $target, [NullString]::Value)
        } else {
            [IO.File]::Move($staged, $target)
        }
        $staged = $null
        $env:PATH = "$destination;$env:PATH"
        Write-Host "Installed h2a $version to $target"
    } finally {
        if ($staged -and (Test-Path -LiteralPath $staged)) { Remove-Item -LiteralPath $staged -Force }
        if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
    }
    if ($env:H2A_INSTALL_ONLY -ne '1') {
        & $target init
        if ($LASTEXITCODE -ne 0) { throw "h2a setup exited with code $LASTEXITCODE" }
    }
}
Install-H2a
