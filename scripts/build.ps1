param([string[]]$ModuleId = @(), [switch]$Test, [switch]$Unsigned, [string]$OutputDirectory = "")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$out = if ($OutputDirectory) { [System.IO.Path]::GetFullPath($OutputDirectory) } else { Join-Path $root "dist" }
New-Item -ItemType Directory -Force -Path $out | Out-Null
$modules = Get-Content -Raw (Join-Path $root "modules.json") | ConvertFrom-Json
foreach ($directory in $modules) {
    $moduleRoot = Join-Path $root $directory
    $manifestPath = Join-Path $moduleRoot "module.json"
    $manifest = Get-Content -Raw $manifestPath | ConvertFrom-Json
    if ($ModuleId.Count -gt 0 -and $manifest.id -notin $ModuleId) { continue }
    Push-Location $moduleRoot
    try {
        if ($Test) {
            & cargo test --locked
            if ($LASTEXITCODE -ne 0) { throw "Tests failed: $directory" }
        }
        & cargo build --locked --target wasm32-wasip2 --release
        if ($LASTEXITCODE -ne 0) { throw "Build failed: $directory" }
        $cargo = Get-Content -Raw "Cargo.toml"
        $lib = [regex]::Match($cargo, '(?s)\[lib\].*?name\s*=\s*"([^"]+)"').Groups[1].Value
        if (-not $lib) { throw "Missing [lib] name: $directory" }
        $wasm = Join-Path $moduleRoot "target/wasm32-wasip2/release/$lib.wasm"
        $package = Join-Path $out "$($manifest.id)-$($manifest.version).anmx"
        if ($Unsigned) {
            Write-Host "Built $($manifest.id); signing skipped."
        } else {
            & cargo run --locked --manifest-path "$root/vendor/sdk/Cargo.toml" -p anmx -- pack $manifestPath $wasm $package
            if ($LASTEXITCODE -ne 0) { throw "Packaging failed: $directory" }
        }
    } finally { Pop-Location }
}
