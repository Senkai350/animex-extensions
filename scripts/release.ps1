#Requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$Publish,
    [string]$SigningKeyFile = (Join-Path ([Environment]::GetFolderPath('UserProfile')) '.animex/signing-key.private')
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$repository = 'Senkai350/animex-extensions'
$identity = 'Senkai350.animex-extensions'
$githubBase = "https://github.com/$repository/releases/"
$previousKey = [Environment]::GetEnvironmentVariable('ANMX_SIGNING_KEY', 'Process')

function Invoke-Checked {
    param([string]$Command, [string[]]$Arguments)
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Command failed (exit $LASTEXITCODE)." }
}

function Assert-PublishSource {
    $changes = @(Invoke-Checked git @('status', '--porcelain', '--untracked-files=all'))
    if ($changes.Count) { throw 'Commit all source changes before publishing. Build without -Publish to verify drafts.' }
    Invoke-Checked git @('fetch', 'origin', 'main')
    $remote = Invoke-Checked gh @('api', "repos/$repository/commits/main", '--jq', '.sha')
    if ($remote -cne $script:sourceCommit) { throw 'Push the source commit to main before publishing; HEAD must match GitHub main.' }
}

Push-Location $root
try {
    $sourceCommit = Invoke-Checked git @('rev-parse', 'HEAD')
    if ($Publish) { Assert-PublishSource }
    if (-not $previousKey) {
        $keyPath = [System.IO.Path]::GetFullPath($SigningKeyFile)
        $relative = [System.IO.Path]::GetRelativePath($root, $keyPath)
        if (-not [System.IO.Path]::IsPathRooted($relative) -and $relative -notmatch '^\.\.[\\/]') {
            throw 'The signing key must be stored outside the repository.'
        }
        if (-not (Test-Path -LiteralPath $keyPath -PathType Leaf)) {
            throw 'Existing signing key required: set ANMX_SIGNING_KEY or supply -SigningKeyFile. Do not rotate the repository key.'
        }
        $env:ANMX_SIGNING_KEY = (Get-Content -LiteralPath $keyPath -Raw).Trim()
    }
    $sdkManifest = Join-Path $root 'vendor/sdk/Cargo.toml'
    Invoke-Checked cargo @('build', '--locked', '--manifest-path', $sdkManifest, '--target-dir', (Join-Path $root 'vendor/sdk/target'), '-p', 'anmx')
    $anmx = Join-Path $root ('vendor/sdk/target/debug/anmx' + $(if ($IsWindows) { '.exe' } else { '' }))
    $publicKey = Invoke-Checked $anmx @('public-key')
    $pin = Get-Content -Raw (Join-Path $root 'repository-key.json') | ConvertFrom-Json
    if ($publicKey -cne $pin.public_key) { throw 'Signing key does not match repository-key.json; publication stopped.' }

    # A new directory for every invocation: old packages never leak into a catalog.
    $stamp = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    $tag = "extensions-$stamp-$($sourceCommit.Substring(0, 12))"
    $output = Join-Path $root "dist-releases/$tag-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
    $github = Join-Path $output 'github'
    $vps = Join-Path $output 'vps'
    New-Item -ItemType Directory -Path $github, $vps | Out-Null
    Invoke-Checked cargo @('test', '--locked', '--manifest-path', $sdkManifest, '--workspace')
    & (Join-Path $PSScriptRoot 'build.ps1') -Test -OutputDirectory $github
    $packages = @(Get-ChildItem -LiteralPath $github -Filter '*.anmx' -File)
    $modules = @(Get-Content -Raw (Join-Path $root 'modules.json') | ConvertFrom-Json)
    if ($packages.Count -ne $modules.Count) { throw 'Incomplete package set; publication stopped.' }

    # Package URLs are immutable per release; the descriptor keeps a stable index URL.
    Invoke-Checked $anmx @('index', $identity, 'AniMex Extensions', "${githubBase}download/$tag/", $github, $github)
    $descriptorPath = Join-Path $github 'repo.json'
    $descriptor = Get-Content -Raw $descriptorPath | ConvertFrom-Json
    $descriptor.index_url = "${githubBase}latest/download/index.json"
    $descriptor | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $descriptorPath -Encoding utf8NoBOM
    $packages | Copy-Item -Destination $vps
    Invoke-Checked $anmx @('index', $identity, 'AniMex Extensions', 'https://senkai.fun/extensions/', $vps, $vps)
    $index = Get-Content -Raw (Join-Path $github 'index.json') | ConvertFrom-Json
    $vpsIndex = Get-Content -Raw (Join-Path $vps 'index.json') | ConvertFrom-Json
    # Compare source before/after the build; never attach binaries to a different commit.
    if ($Publish) {
        Assert-PublishSource
        $releases = @(Invoke-Checked gh @('api', "repos/$repository/releases?per_page=100") | ConvertFrom-Json)
        $latest = $releases | Where-Object { -not $_.draft -and -not $_.prerelease } |
            Sort-Object published_at -Descending | Select-Object -First 1
        if ($latest) {
            $oldPath = Join-Path $output 'previous'
            New-Item -ItemType Directory -Path $oldPath | Out-Null
            Invoke-Checked gh @('release', 'download', $latest.tag_name, '--repo', $repository, '--pattern', 'index.json', '--dir', $oldPath)
            $old = Get-Content -Raw (Join-Path $oldPath 'index.json') | ConvertFrom-Json
            if ($index.revision -le $old.revision) { throw 'Catalog revision must increase; check the system clock.' }
            foreach ($entry in $index.modules) {
                $previous = $old.modules | Where-Object { $_.manifest.id -ceq $entry.manifest.id }
                if ($previous -and [System.Management.Automation.SemanticVersion]::Parse($entry.manifest.version) -lt [System.Management.Automation.SemanticVersion]::Parse($previous.manifest.version)) {
                    throw "Version rollback: $($entry.manifest.id). Increase module.json version and rebuild."
                }
                if ($previous -and $previous.manifest.version -ceq $entry.manifest.version -and $previous.sha256 -cne $entry.sha256) {
                    throw "Package bytes changed without a version bump: $($entry.manifest.id). Increase module.json version and rebuild."
                }
            }
        }
        Copy-Item -LiteralPath (Join-Path $vps 'repo.json') -Destination (Join-Path $github 'repo-vps.json')
        Copy-Item -LiteralPath (Join-Path $vps 'index.json') -Destination (Join-Path $github 'index-vps.json')
        $assets = @(Get-ChildItem -LiteralPath $github -File | ForEach-Object FullName)
        $notes = Join-Path $output 'release-notes.md'
        @(
            "Source: https://github.com/$repository/commit/$sourceCommit"
            "GitHub catalog: ${githubBase}latest/download/repo.json"
            'VPS catalog: https://senkai.fun/extensions/repo.json'
            ''
            ($index.modules | ForEach-Object { "- $($_.manifest.id) $($_.manifest.version)" })
        ) | Set-Content -LiteralPath $notes -Encoding utf8NoBOM
        # A failed upload remains a draft; the previous latest release remains usable.
        Invoke-Checked gh (@('release', 'create', $tag, '--repo', $repository, '--target', $sourceCommit,
            '--draft', '--title', "Extensions $stamp", '--notes-file', $notes) + $assets)
        Invoke-Checked gh @('release', 'edit', $tag, '--repo', $repository, '--draft=false', '--latest')
        Write-Host "Published: ${githubBase}tag/$tag"
        Write-Host "Connect AniMex: ${githubBase}latest/download/repo.json"
    }
    Write-Host "Signed catalogs and $($packages.Count) packages: $output"
    if (-not $Publish) { Write-Host 'Local build only. Commit/push sources, then run scripts/release.ps1 -Publish to build and publish.' }
} finally {
    [Environment]::SetEnvironmentVariable('ANMX_SIGNING_KEY', $previousKey, 'Process')
    Pop-Location
}
