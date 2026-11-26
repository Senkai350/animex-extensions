#Requires -Version 7.0
# Exercise publication order and failure handling with real signatures, no network.
$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) "animex-release-test-$([guid]::NewGuid().ToString('N'))"
$originalKey = $env:ANMX_SIGNING_KEY
$global:releaseCalls = [System.Collections.Generic.List[string]]::new()
$global:releaseScenario = ''
$global:releaseStatusCalls = 0
$global:releaseHead = 'a' * 40

function global:git {
    $global:LASTEXITCODE = 0
    if ($args[0] -eq 'rev-parse') { return $global:releaseHead }
    if ($args[0] -eq 'status') {
        $global:releaseStatusCalls++
        if ($global:releaseScenario -eq 'dirty' -or
            ($global:releaseScenario -eq 'changed-during-build' -and $global:releaseStatusCalls -gt 1)) {
            return ' M modules/test/module.json'
        }
    }
}

function global:cargo { $global:LASTEXITCODE = 0 }

function global:gh {
    $global:LASTEXITCODE = 0
    $global:releaseCalls.Add(($args -join ' '))
    if ($args[0] -eq 'api') {
        if ($args[1] -like '*/commits/main') {
            if ($global:releaseScenario -eq 'unpublished-source') { return ('b' * 40) }
            return $global:releaseHead
        }
        if ($global:releaseScenario -in @('same-version-unchanged', 'changed-without-version', 'version-rollback', 'revision-conflict')) {
            return '[{"tag_name":"extensions-old","published_at":"2026-11-24T00:00:00Z","draft":false,"prerelease":false}]'
        }
        return '[]'
    }
    if ($args[1] -eq 'download') {
        $destination = $args[[array]::IndexOf($args, '--dir') + 1]
        $previousHash = if ($global:releaseScenario -eq 'same-version-unchanged') {
            $global:releaseFixtureHash
        } else { '0' * 64 }
        @{revision=([DateTimeOffset]::UtcNow.ToUnixTimeSeconds() + $(if ($global:releaseScenario -eq 'revision-conflict') { 3600 } else { -100 }));
          modules=@(@{manifest=@{id='source.test';version=$(if ($global:releaseScenario -eq 'version-rollback') { '2.0.0' } else { '1.0.0' })};sha256=$previousHash})} |
            ConvertTo-Json -Depth 10 | Set-Content (Join-Path $destination 'index.json') -Encoding utf8NoBOM
    }
    if ($args[1] -eq 'create' -and $global:releaseScenario -eq 'failed-upload') {
        $global:LASTEXITCODE = 1
    }
}

try {
    New-Item -ItemType Directory -Path "$testRoot/scripts", "$testRoot/vendor/sdk/target/debug" | Out-Null
    Copy-Item -LiteralPath (Join-Path $repositoryRoot 'scripts/release.ps1') -Destination "$testRoot/scripts/"
    $executable = 'anmx' + $(if ($IsWindows) { '.exe' } else { '' })
    Copy-Item -LiteralPath (Join-Path $repositoryRoot "vendor/sdk/target/debug/$executable") -Destination "$testRoot/vendor/sdk/target/debug/"
    $anmx = Join-Path $testRoot "vendor/sdk/target/debug/$executable"
    $env:ANMX_SIGNING_KEY = 'ed25519:' + [Convert]::ToBase64String([byte[]]::new(32))
    $pin = & $anmx public-key
    @{public_key=$pin} | ConvertTo-Json | Set-Content "$testRoot/repository-key.json" -Encoding utf8NoBOM
    '["modules/test"]' | Set-Content "$testRoot/modules.json" -Encoding utf8NoBOM
    @'
param([switch]$Test, [string]$OutputDirectory)
$root = Split-Path -Parent $PSScriptRoot
$anmx = Join-Path $root ('vendor/sdk/target/debug/anmx' + $(if ($IsWindows) { '.exe' } else { '' }))
$manifest = Join-Path $root 'fixture.json'
@{id='source.test';kind='source';name='Test';version='1.0.0';api_version=3;
  author=@{name='Test';public_key='replaced by pack'};released_at='2026-11-24T00:00:00Z';entry='module.wasm'} |
    ConvertTo-Json | Set-Content $manifest -Encoding utf8NoBOM
$wasm = Join-Path $root 'fixture.wasm'
[System.IO.File]::WriteAllBytes($wasm, [byte[]](0,97,115,109,1,0,0,0))
& $anmx pack $manifest $wasm (Join-Path $OutputDirectory 'source.test-1.0.0.anmx')
if ($LASTEXITCODE -ne 0) { throw 'Fixture signing failed' }
'@ | Set-Content "$testRoot/scripts/build.ps1" -Encoding utf8NoBOM
    & "$testRoot/scripts/build.ps1" -OutputDirectory $testRoot
    $global:releaseFixtureHash = (Get-FileHash -Algorithm SHA256 "$testRoot/source.test-1.0.0.anmx").Hash.ToLowerInvariant()
    foreach ($scenario in @('success', 'dirty', 'unpublished-source', 'changed-during-build', 'failed-upload', 'wrong-key',
                            'same-version-unchanged', 'changed-without-version', 'version-rollback', 'revision-conflict')) {
        $global:releaseScenario = $scenario
        $global:releaseStatusCalls = 0
        $global:releaseCalls.Clear()
        @{public_key=$(if ($scenario -eq 'wrong-key') { 'wrong' } else { $pin })} |
            ConvertTo-Json | Set-Content "$testRoot/repository-key.json" -Encoding utf8NoBOM
        $failed = $false
        $failureMessage = ''
        try { & "$testRoot/scripts/release.ps1" -Publish }
        catch { $failed = $true; $failureMessage = $_.Exception.Message }
        $shouldSucceed = $scenario -in @('success', 'same-version-unchanged')
        if ($failed -eq $shouldSucceed) { throw "Unexpected result for ${scenario}: $failureMessage" }
        $expectedErrors = @{
            dirty='Commit all source changes*'; 'unpublished-source'='Push the source commit*';
            'changed-during-build'='Commit all source changes*'; 'failed-upload'='gh failed*';
            'wrong-key'='Signing key does not match*'; 'changed-without-version'='Package bytes changed without a version bump*';
            'version-rollback'='Version rollback*'; 'revision-conflict'='Catalog revision must increase*'
        }
        if ($failed -and $failureMessage -notlike $expectedErrors[$scenario]) { throw "Wrong failure for ${scenario}: $failureMessage" }
        $created = @($global:releaseCalls | Where-Object { $_ -like 'release create *' })
        $published = @($global:releaseCalls | Where-Object { $_ -like 'release edit *' })
        if ($shouldSucceed) {
            if ($created.Count -ne 1 -or $published.Count -ne 1 -or $created[0] -notlike '*--draft *' -or
                $published[0] -notlike '*--draft=false --latest' -or
                $created[0] -notlike '*repo-vps.json*' -or $created[0] -notlike '*source.test-1.0.0.anmx*') {
                throw 'Publication must upload the complete set as draft before updating Latest'
            }
        } elseif ($published.Count) { throw "Failed build changed Latest: $scenario" }
        if (-not $shouldSucceed -and $scenario -ne 'failed-upload' -and $created.Count) { throw "Unsafe draft created: $scenario" }
        if ($env:ANMX_SIGNING_KEY -cne ('ed25519:' + [Convert]::ToBase64String([byte[]]::new(32)))) { throw 'Signing environment was not restored' }
        Write-Host "PASS release: $scenario"
    }
} finally {
    [Environment]::SetEnvironmentVariable('ANMX_SIGNING_KEY', $originalKey, 'Process')
    Remove-Item Function:\git, Function:\cargo, Function:\gh
    Remove-Variable -Name releaseFixtureHash -Scope Global -ErrorAction SilentlyContinue
    $resolvedTestRoot = [System.IO.Path]::GetFullPath($testRoot)
    $tempParent = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    if ($resolvedTestRoot.StartsWith($tempParent) -and (Split-Path -Leaf $resolvedTestRoot) -like 'animex-release-test-*') {
        Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
    }
}
