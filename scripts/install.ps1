# Install a released Windows x64 distribution. Works in Windows PowerShell 5.1 and PowerShell 7.
# $env:ARCHGUARD_VERSION = '0.2.0'; irm <tagged-script-url> | iex
param(
    [string]$Version = $env:ARCHGUARD_VERSION,
    [string]$InstallDirectory,
    [switch]$NoPath
)
$ErrorActionPreference = 'Stop'
$work = $null
try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'This installer requires Windows.' }
    $build = [int](Get-CimInstance -ClassName Win32_OperatingSystem).BuildNumber
    if ($build -lt 17763) { throw 'This release requires Windows 10 1809 or Windows Server 2019 or newer.' }
    $architecture = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    if ($architecture -ne 'AMD64') { throw 'This release supports native Windows x64 only.' }
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    if (-not $Version) {
        $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/alundgren/irudd-ts/releases/latest'
        $Version = $release.tag_name -replace '^v', ''
    }
    if ($Version -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') { throw 'Expected an exact stable version such as 0.2.0.' }
    if (-not $InstallDirectory) { $InstallDirectory = Join-Path $env:LOCALAPPDATA "Archguard\$Version" }
    $destination = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($InstallDirectory)
    if (Test-Path -LiteralPath $destination) { throw "Install directory already exists: $destination" }
    $parent = [IO.Path]::GetDirectoryName($destination)
    [IO.Directory]::CreateDirectory($parent) | Out-Null
    $work = Join-Path $parent ('.archguard-install-' + [Guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($work) | Out-Null
    $archiveName = "archguard-$Version-windows-x86_64.zip"
    $base = if ($env:ARCHGUARD_DOWNLOAD_BASE_URL) { $env:ARCHGUARD_DOWNLOAD_BASE_URL.TrimEnd('/') } else { "https://github.com/alundgren/irudd-ts/releases/download/v$Version" }
    foreach ($name in @($archiveName, "$archiveName.sha256")) {
        $uri = [Uri]"$base/$name"
        if ($uri.Scheme -eq 'file') {
            [IO.File]::Copy($uri.LocalPath, (Join-Path $work $name))
        } elseif ($uri.Scheme -eq 'https') {
            Invoke-WebRequest -UseBasicParsing -Uri $uri -OutFile (Join-Path $work $name)
        } else { throw 'Downloads require HTTPS or a local file URL.' }
    }
    $archivePath = Join-Path $work $archiveName
    $checksum = [IO.File]::ReadAllText("$archivePath.sha256")
    $pattern = '\A([a-f0-9]{64})  ' + [Regex]::Escape($archiveName) + '\r?\n\z'
    if ($checksum -cnotmatch $pattern) { throw 'Invalid archive checksum record.' }
    $expected = $Matches[1]
    if ((Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected) { throw 'Archive checksum mismatch.' }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archivePath)
    $rootName = "archguard-$Version"
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    try {
        # Validate every path before extracting any bytes. Zip entries must be regular files.
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName
            $parts = $name.Split('/')
            $type = ($entry.ExternalAttributes -shr 16) -band 0xF000
            if (-not $name.StartsWith("$rootName/", [StringComparison]::Ordinal) -or
                $name.Contains('\') -or $name.Contains(':') -or $name.Contains([char]0) -or
                ($type -ne 0 -and $type -ne 0x8000) -or ($entry.ExternalAttributes -band 0x400) -ne 0 -or
                -not $seen.Add($name)) { throw 'Unsafe archive entry.' }
            foreach ($part in $parts) {
                if (-not $part -or $part -eq '.' -or $part -eq '..' -or $part -match '[<>"|?*\x00-\x1f]' -or
                    $part.EndsWith('.') -or $part.EndsWith(' ') -or $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') { throw 'Unsafe archive path.' }
            }
        }
    } finally { $zip.Dispose() }
    [IO.Compression.ZipFile]::ExtractToDirectory($archivePath, $work)
    $root = Join-Path $work $rootName
    if ([IO.File]::ReadAllText((Join-Path $root 'VERSION')).Trim() -cne $Version) { throw 'Archive version mismatch.' }
    $identity = [IO.File]::ReadAllText((Join-Path $root 'release.json')) | ConvertFrom-Json
    if ($identity.version -cne $Version -or $identity.platform -cne 'windows-x86_64') { throw 'Archive platform identity mismatch.' }
    $binary = Join-Path $root 'bin\archguard.exe'
    $binaryVersion = & $binary --version
    if ($LASTEXITCODE -ne 0 -or $binaryVersion -cne "archguard $Version") { throw 'Binary version mismatch.' }
    [IO.Directory]::Move($root, $destination)
    if (-not $NoPath) {
        $bin = Join-Path $destination 'bin'
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if ($bin -notin ($userPath -split ';')) {
            [Environment]::SetEnvironmentVariable('Path', (($bin, $userPath | Where-Object { $_ }) -join ';'), 'User')
        }
        $env:Path = "$bin;$env:Path"
    }
    Write-Output $destination
} finally {
    if ($work -and (Test-Path -LiteralPath $work)) { Remove-Item -LiteralPath $work -Recurse -Force }
}
