# Native validation shared by premerge checks and the manual release publisher.
param([string]$OutputDirectory = 'dist')
$ErrorActionPreference = 'Stop'
# Mutation inventories hash the CLI; omit debugger data from acceptance binaries.
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_PROFILE_TEST_DEBUG = '0'
# Inventory hashing also reads external executables such as Node.
$env:CARGO_PROFILE_DEV_OPT_LEVEL = '1'
$env:CARGO_PROFILE_TEST_OPT_LEVEL = '1'
function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit $LASTEXITCODE" }
}
Invoke-Checked rustup -Arguments @('toolchain', 'install', '1.96.0', '--profile', 'minimal', '--component', 'rust-docs', '--component', 'llvm-tools-preview')
Invoke-Checked npm.cmd -Arguments @('ci', '--prefix', 'providers/typescript7', '--ignore-scripts', '--no-audit', '--no-fund')
. (Join-Path $PSScriptRoot 'release/windows_notices.ps1')
$windowsToolchain = Initialize-WindowsToolchain
$releaseBinary = 'target/x86_64-pc-windows-msvc/release/archguard.exe'
$savedRustFlags = $env:RUSTFLAGS
$savedTargetLinker = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER
try {
    $env:RUSTFLAGS = '-C target-feature=+crt-static'
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = $windowsToolchain.Linker
    Invoke-Checked cargo -Arguments @('+1.96.0', 'build', '--release', '--locked', '--target', 'x86_64-pc-windows-msvc', '--bin', 'archguard')
} finally {
    $env:RUSTFLAGS = $savedRustFlags
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = $savedTargetLinker
}
$sysroot = & rustc +1.96.0 --print sysroot
if ($LASTEXITCODE -ne 0) { throw 'Cannot locate the pinned Rust toolchain' }
$readObject = Join-Path $sysroot 'lib/rustlib/x86_64-pc-windows-msvc/bin/llvm-readobj.exe'
function Assert-StandaloneRuntime {
    param([string]$Binary)
    $imports = & $readObject --coff-imports $Binary
    if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect Windows executable imports' }
    if (($imports -join "`n") -match '(?im)^\s*Name:\s*(?:vcruntime|msvcp|msvcr|concrt)\d[\w.-]*\.dll\s*$') {
        throw 'Windows executable requires a separate Visual C++ runtime'
    }
}
$probeDirectory = Join-Path ([IO.Path]::GetTempPath()) ('archguard-runtime-probe-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($probeDirectory) | Out-Null
try {
    $probeSource = Join-Path $probeDirectory 'probe.rs'
    [IO.File]::WriteAllText($probeSource, 'fn main() { println!("{}", std::env::args().count()); }')
    Invoke-Checked rustc -Arguments @('+1.96.0', '--target', 'x86_64-pc-windows-msvc', '-C', 'target-feature=-crt-static', '-C', ('linker=' + $windowsToolchain.Linker), $probeSource, '-o', (Join-Path $probeDirectory 'probe.exe'))
    $rejected = $false
    try { Assert-StandaloneRuntime (Join-Path $probeDirectory 'probe.exe') } catch {
        if ($_.Exception.Message -ne 'Windows executable requires a separate Visual C++ runtime') { throw }
        $rejected = $true
    }
    if (-not $rejected) { throw 'Dynamic-runtime rejection control did not fail' }
    Assert-StandaloneRuntime $releaseBinary
} finally { Remove-Item -LiteralPath $probeDirectory -Recurse -Force }
Invoke-Checked cargo -Arguments @('+1.96.0', 'test', '--locked', '--features', 'semantic-tests', '--no-fail-fast')
Invoke-Checked node -Arguments @('--test', 'tests/mutator_sdk.test.ts', 'tests/mutator_reporter.test.ts')
$runtimeNotices = Join-Path ([IO.Path]::GetTempPath()) ('archguard-runtime-notices-' + [Guid]::NewGuid().ToString('N'))
try {
    Export-WindowsRuntimeNotices -Binary $releaseBinary -Destination $runtimeNotices
    Invoke-Checked python -Arguments @('scripts/release/package.py', '--binary', $releaseBinary, '--platform', 'windows-x86_64', '--output', $OutputDirectory, '--windows-runtime-notices', $runtimeNotices)
} finally {
    if (Test-Path -LiteralPath $runtimeNotices) { Remove-Item -LiteralPath $runtimeNotices -Recurse -Force }
}
Invoke-Checked powershell -Arguments @('-NoProfile', '-File', 'tests/windows_installer.ps1', '-DistributionDirectory', $OutputDirectory)
if ($env:GITHUB_OUTPUT) {
    $version = (& $releaseBinary --version) -replace '^archguard ', ''
    $fixture = Join-Path $env:RUNNER_TEMP ('archguard-action-smoke-' + [Guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($fixture) | Out-Null
    [IO.File]::WriteAllText((Join-Path $fixture 'source.ts'), 'export const answer = 42;')
    [IO.File]::WriteAllText((Join-Path $fixture 'archguard.json'), '{"schemaVersion":1,"include":["*.ts"],"rules":[]}')
    $download = ([Uri]((Resolve-Path $OutputDirectory).Path + [IO.Path]::DirectorySeparatorChar)).AbsoluteUri.TrimEnd('/')
    "version=$version", "action_root=$fixture", "download_base=$download" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
}
