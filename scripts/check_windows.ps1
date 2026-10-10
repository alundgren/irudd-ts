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
Invoke-Checked rustup -Arguments @('toolchain', 'install', '1.96.0', '--profile', 'minimal', '--component', 'rust-docs')
Invoke-Checked npm.cmd -Arguments @('ci', '--prefix', 'providers/typescript7', '--ignore-scripts', '--no-audit', '--no-fund')
Invoke-Checked cargo -Arguments @('+1.96.0', 'build', '--release', '--locked', '--bin', 'archguard')
Invoke-Checked cargo -Arguments @('+1.96.0', 'test', '--locked', '--features', 'semantic-tests', '--no-fail-fast')
Invoke-Checked node -Arguments @('--test', 'tests/mutator_sdk.test.ts', 'tests/mutator_reporter.test.ts')
Invoke-Checked python -Arguments @('scripts/release/package.py', '--binary', 'target/release/archguard.exe', '--platform', 'windows-x86_64', '--output', $OutputDirectory)
Invoke-Checked powershell -Arguments @('-NoProfile', '-File', 'tests/windows_installer.ps1', '-DistributionDirectory', $OutputDirectory)
if ($env:GITHUB_OUTPUT) {
    $version = (& target/release/archguard.exe --version) -replace '^archguard ', ''
    $fixture = Join-Path $env:RUNNER_TEMP ('archguard-action-smoke-' + [Guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($fixture) | Out-Null
    [IO.File]::WriteAllText((Join-Path $fixture 'source.ts'), 'export const answer = 42;')
    [IO.File]::WriteAllText((Join-Path $fixture 'archguard.json'), '{"schemaVersion":1,"include":["*.ts"],"rules":[]}')
    $download = ([Uri]((Resolve-Path $OutputDirectory).Path + [IO.Path]::DirectorySeparatorChar)).AbsoluteUri.TrimEnd('/')
    "version=$version", "action_root=$fixture", "download_base=$download" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
}
