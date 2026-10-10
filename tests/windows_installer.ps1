# Exercise the real PowerShell installer with a native release archive.
param([Parameter(Mandatory = $true)][string]$DistributionDirectory)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$work = Join-Path ([IO.Path]::GetTempPath()) ('archguard-installer-test-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($work) | Out-Null
try {
    $source = Get-Item (Join-Path $DistributionDirectory 'archguard-*-windows-x86_64.zip')
    $version = $source.Name -replace '^archguard-(.+)-windows-x86_64.zip$', '$1'
    $archive = Join-Path $work $source.Name
    [IO.File]::Copy($source.FullName, $archive)
    $checksumPath = "$archive.sha256"
    function Write-Checksum {
        $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        [IO.File]::WriteAllText($checksumPath, "$hash  $($source.Name)`n")
    }
    $env:ARCHGUARD_DOWNLOAD_BASE_URL = ([Uri]($work + [IO.Path]::DirectorySeparatorChar)).AbsoluteUri.TrimEnd('/')
    $installer = Join-Path $PSScriptRoot '..\scripts\install.ps1'
    function Install-Control {
        param([string]$Name, [bool]$Succeeds, [string]$ExpectedError)
        $destination = Join-Path $work $Name
        $ErrorActionPreference = 'Continue'
        $output = & powershell -NoProfile -File $installer -Version $version -InstallDirectory $destination -NoPath 2>&1
        $code = $LASTEXITCODE
        $ErrorActionPreference = 'Stop'
        if (($code -eq 0) -ne $Succeeds) { throw "Installer $Name expected success=$Succeeds, exit=$code output=$output" }
        if ($ExpectedError -and ($output -join "`n") -notmatch [Regex]::Escape($ExpectedError)) { throw "Installer $Name did not reject the archive during path validation: $output" }
        if (-not $Succeeds -and $Name -ne 'installed' -and (Test-Path -LiteralPath $destination)) { throw 'Failed install published a destination.' }
    }
    Write-Checksum
    $goodChecksum = [IO.File]::ReadAllText($checksumPath)
    [IO.File]::WriteAllText($checksumPath, ('0' * 64) + "  $($source.Name)`n")
    Install-Control 'bad-checksum' $false
    [IO.File]::WriteAllText($checksumPath, $goodChecksum)
    Install-Control 'installed' $true
    Install-Control 'installed' $false
    $binary = Join-Path $work 'installed\bin\archguard.exe'
    & python (Join-Path $PSScriptRoot '..\scripts\release\smoke.py') $binary
    if ($LASTEXITCODE -ne 0) { throw 'Installed executable smoke failed.' }
    foreach ($control in @('traversal', 'symlink', 'duplicate', 'wrong-version', 'device-name', 'console-name')) {
        [IO.File]::Copy($source.FullName, $archive, $true)
        $zip = [IO.Compression.ZipFile]::Open($archive, [IO.Compression.ZipArchiveMode]::Update)
        try {
            if ($control -eq 'wrong-version') {
                $zip.GetEntry("archguard-$version/VERSION").Delete()
                $entry = $zip.CreateEntry("archguard-$version/VERSION")
                $writer = [IO.StreamWriter]::new($entry.Open())
                try { $writer.Write('999.0.0') } finally { $writer.Dispose() }
            } else {
                $name = switch ($control) {
                    'traversal' { "archguard-$version/../../escaped" }
                    'symlink' { "archguard-$version/link" }
                    'duplicate' { "archguard-$version/version" }
                    'device-name' { "archguard-$version/COM$([char]0x00b9).txt" }
                    'console-name' { 'archguard-' + $version + '/CONOUT$' }
                }
                $entry = $zip.CreateEntry($name)
                if ($control -eq 'symlink') { $entry.ExternalAttributes = -1610612736 }
            }
        } finally { $zip.Dispose() }
        Write-Checksum
        $expectedError = if ($control -in @('device-name', 'console-name')) { 'Unsafe archive path.' } else { $null }
        Install-Control $control $false $expectedError
    }
    [IO.File]::Copy($source.FullName, $archive, $true)
    Write-Checksum
    Install-Control 'corrected' $true
    $previousUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $previousProcessPath = $env:Path
    $previousLocation = Get-Location
    try {
        Set-Location -LiteralPath $work
        $relativeDestination = & $installer -Version $version -InstallDirectory 'relative-installed' -NoPath
        if ($relativeDestination -ne (Join-Path $work 'relative-installed')) { throw 'Relative install directory ignored PowerShell location.' }
        $pathDestination = & $installer -Version $version -InstallDirectory 'path-installed'
        $expectedBin = Join-Path $pathDestination 'bin'
        if ($expectedBin -notin ($env:Path -split ';') -or
            $expectedBin -notin ([Environment]::GetEnvironmentVariable('Path', 'User') -split ';')) { throw 'Installer did not update user and current PATH.' }
    } finally {
        Set-Location -LiteralPath $previousLocation.Path
        [Environment]::SetEnvironmentVariable('Path', $previousUserPath, 'User')
        $env:Path = $previousProcessPath
    }
    if (Test-Path -LiteralPath (Join-Path $work 'escaped')) { throw 'Unsafe archive wrote outside extraction.' }
    Write-Output 'Windows installer failure, correction, and installed exit controls passed.'
} finally { Remove-Item -LiteralPath $work -Recurse -Force }
