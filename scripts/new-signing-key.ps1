param([string]$OutputPath = (Join-Path ([Environment]::GetFolderPath("UserProfile")) ".animex/signing-key.private"))
$ErrorActionPreference = "Stop"
$absolute = [System.IO.Path]::GetFullPath($OutputPath)
if (Test-Path -LiteralPath $absolute) { throw "Refusing to overwrite an existing key: $absolute" }
$parent = Split-Path -Parent $absolute
New-Item -ItemType Directory -Force -Path $parent | Out-Null
$bytes = New-Object byte[] 32
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
try {
    $rng.GetBytes($bytes)
    $encoded = "ed25519:" + [Convert]::ToBase64String($bytes)
    $file = [System.IO.File]::Open($absolute, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write)
    try {
        $encodedBytes = [System.Text.Encoding]::UTF8.GetBytes($encoded)
        $file.Write($encodedBytes, 0, $encodedBytes.Length)
        [Array]::Clear($encodedBytes, 0, $encodedBytes.Length)
    } finally { $file.Dispose() }
} finally {
    $rng.Dispose()
    [Array]::Clear($bytes, 0, $bytes.Length)
    $encoded = $null
}
Write-Host "Private signing key saved to $absolute. Keep it outside Git and back it up securely."
