param(
    [string[]]$ModuleId = @(),
    [switch]$Test,
    [switch]$Unsigned,
    [string]$OutputDirectory = "",
    [string]$DevPackExecutable,
    [string]$DevSigningSeed
)
$ErrorActionPreference = "Stop"
if ($PSBoundParameters.ContainsKey("DevPackExecutable") -or $PSBoundParameters.ContainsKey("DevSigningSeed")) {
    # Internal sync mode: the seed is passed only between scripts in the same PowerShell process.
    if ($Unsigned -or -not $DevPackExecutable -or -not $DevSigningSeed -or
        -not [System.IO.Path]::IsPathRooted($DevPackExecutable) -or
        [System.IO.Path]::GetFileName($DevPackExecutable) -cne "anmx.exe" -or
        -not [System.IO.File]::Exists($DevPackExecutable) -or (Test-Path Env:ANMX_SIGNING_KEY)) {
        throw "Dev packaging requires an absolute anmx.exe path, an in-process seed, and no inherited signing key."
    }
}
$root = Split-Path -Parent $PSScriptRoot
$out = if ($OutputDirectory) { [System.IO.Path]::GetFullPath($OutputDirectory) } else { Join-Path $root "dist" }
New-Item -ItemType Directory -Force -Path $out | Out-Null
$modules = Get-Content -Encoding UTF8 -Raw (Join-Path $root "modules.json") | ConvertFrom-Json
foreach ($directory in $modules) {
    $moduleRoot = Join-Path $root $directory
    $manifestPath = Join-Path $moduleRoot "module.json"
    $manifest = Get-Content -Encoding UTF8 -Raw $manifestPath | ConvertFrom-Json
    if ($ModuleId.Count -gt 0 -and $manifest.id -cnotin $ModuleId) { continue }
    Push-Location $moduleRoot
    try {
        if ($Test) {
            & cargo test --locked
            if ($LASTEXITCODE -ne 0) { throw "Tests failed: $directory" }
        }
        & cargo build --locked --target-dir (Join-Path $moduleRoot "target") --target wasm32-wasip2 --release
        if ($LASTEXITCODE -ne 0) { throw "Build failed: $directory" }
        $cargo = Get-Content -Raw "Cargo.toml"
        $lib = [regex]::Match($cargo, '(?s)\[lib\].*?name\s*=\s*"([^"]+)"').Groups[1].Value
        if (-not $lib) { throw "Missing [lib] name: $directory" }
        $wasm = Join-Path $moduleRoot "target/wasm32-wasip2/release/$lib.wasm"
        $package = Join-Path $out "$($manifest.id)-$($manifest.version).anmx"
        if ($Unsigned) {
            Write-Host "Built $($manifest.id); signing skipped."
        } elseif ($DevPackExecutable) {
            try {
                $env:ANMX_SIGNING_KEY = $DevSigningSeed
                & $DevPackExecutable pack $manifestPath $wasm $package
            } finally {
                [Environment]::SetEnvironmentVariable("ANMX_SIGNING_KEY", $null, 'Process')
            }
            if ($LASTEXITCODE -ne 0) { throw "Packaging failed: $directory" }
        } else {
            & cargo run --locked --manifest-path "$root/vendor/sdk/Cargo.toml" -p anmx -- pack $manifestPath $wasm $package
            if ($LASTEXITCODE -ne 0) { throw "Packaging failed: $directory" }
        }
    } finally { Pop-Location }
}
