# Initialize the exact toolchain used for the release and retain its runtime notices.
$ErrorActionPreference = 'Stop'
function Initialize-WindowsToolchain {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $raw = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -format json -utf8
    if ($LASTEXITCODE -ne 0) { throw 'vswhere could not identify the release compiler' }
    $installations = @($raw | ConvertFrom-Json)
    if ($installations.Count -ne 1) { throw 'Expected exactly one selected Visual Studio installation' }
    $selected = $installations[0]
    if ($selected.installationVersion -notmatch '^17\.' -or $selected.productId -notin @('Microsoft.VisualStudio.Product.Enterprise', 'Microsoft.VisualStudio.Product.Professional')) {
        throw "Runtime notices do not support Visual Studio $($selected.productId) $($selected.installationVersion)"
    }
    $developerCommand = Join-Path $selected.installationPath 'Common7\Tools\VsDevCmd.bat'
    if (!(Test-Path -LiteralPath $developerCommand -PathType Leaf) -or $developerCommand -match '["%&|<>\r\n]') { throw 'Invalid Visual Studio developer command path' }
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $env:ComSpec
    $start.Arguments = '/d /s /c ""' + $developerCommand + '" -arch=x64 -host_arch=x64 >nul && set"'
    $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    try {
        [void]$process.Start()
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (!$process.WaitForExit(30000)) { $process.Kill(); throw 'Visual Studio environment initialization timed out' }
        $captured = $stdout.GetAwaiter().GetResult()
        [void]$stderr.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0 -or $captured.Length -gt 1048576) { throw 'Visual Studio environment initialization failed' }
        $allowed = @('PATH', 'INCLUDE', 'LIB', 'LIBPATH', 'VSINSTALLDIR', 'VCToolsInstallDir', 'VCToolsVersion', 'WindowsSdkDir', 'WindowsSDKVersion', 'WindowsSDKLibVersion', 'UniversalCRTSdkDir', 'UCRTVersion', 'VSCMD_ARG_TGT_ARCH', 'VSCMD_ARG_HOST_ARCH')
        $imported = @{}
        foreach ($line in ($captured -split "`r?`n")) {
            $parts = $line -split '=', 2
            if ($parts.Count -eq 2 -and $parts[0] -in $allowed) {
                [Environment]::SetEnvironmentVariable($parts[0], $parts[1], 'Process')
                $imported[$parts[0]] = $true
            }
        }
        foreach ($key in @('PATH', 'INCLUDE', 'LIB', 'VSINSTALLDIR', 'VCToolsInstallDir', 'VCToolsVersion', 'WindowsSdkDir', 'WindowsSDKVersion')) {
            if (!$imported.ContainsKey($key) -or ![Environment]::GetEnvironmentVariable($key, 'Process')) { throw "Visual Studio initialization omitted $key" }
        }
        if ($env:VSCMD_ARG_TGT_ARCH -ne 'x64' -or $env:VSCMD_ARG_HOST_ARCH -ne 'x64') { throw 'Visual Studio did not select the x64 release toolchain' }
        $linker = Join-Path $env:VCToolsInstallDir 'bin\Hostx64\x64\link.exe'
        if (!(Test-Path -LiteralPath $linker -PathType Leaf)) { throw 'Selected release linker is missing' }
        $selected | Add-Member -NotePropertyName releaseLinkerPath -NotePropertyValue $linker -Force
        $script:ArchguardWindowsToolchain = $selected
        return @{ productId = $selected.productId; installationVersion = $selected.installationVersion; toolsetVersion = $env:VCToolsVersion; sdkVersion = $env:WindowsSDKVersion; Linker = $linker }
    } finally {
        $captured = $null
        $process.Dispose()
    }
}
function Export-WindowsRuntimeNotices {
    param([Parameter(Mandatory = $true)][string]$Binary, [Parameter(Mandatory = $true)][string]$Destination)
    if (!$script:ArchguardWindowsToolchain) { throw 'Initialize-WindowsToolchain must precede the release build' }
    $identity = [IO.Path]::GetTempFileName()
    try {
        $script:ArchguardWindowsToolchain | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $identity -Encoding UTF8
        & python (Join-Path $PSScriptRoot 'windows_notices.py') --binary $Binary --destination $Destination --visual-studio $identity
        if ($LASTEXITCODE -ne 0) { throw 'Windows runtime notice collection failed' }
    } finally { Remove-Item -LiteralPath $identity -Force -ErrorAction SilentlyContinue }
}
