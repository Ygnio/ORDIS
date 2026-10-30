param(
    [Parameter(Mandatory=$true)][string]$SyntheticLocalAppData,
    [switch]$Cli,
    [Parameter(ValueFromRemainingArguments=$true)][string[]]$CliArguments
)
$ErrorActionPreference = 'Stop'
$local = (Resolve-Path -LiteralPath $SyntheticLocalAppData).Path
if ((Get-Content -LiteralPath (Join-Path $local 'synthetic-staging.txt') -Raw).TrimEnd([char[]]"`r`n") -cne 'ORDIS synthetic staging') {
    throw 'Staging requires an explicitly prepared synthetic profile.'
}
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'manifest.json') -Raw | ConvertFrom-Json
if (-not $manifest.staging -or $manifest.distribution -ne 'lucille') { throw 'Not a Lucille staging package.' }
foreach ($entry in $manifest.files) {
    $path = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot $entry.path))
    if (-not $path.StartsWith($PSScriptRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Invalid manifest path.' }
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ine $entry.sha256) { throw 'Package integrity failed.' }
}
$old = @{}
foreach ($key in @('LOCALAPPDATA','APPDATA','ORDIS_PROJECT','LUCILLE_STAGING_ROOT')) { $old[$key] = [Environment]::GetEnvironmentVariable($key, 'Process') }
try {
    $env:LOCALAPPDATA = $local
    $env:APPDATA = Join-Path $local 'roaming'
    $env:ORDIS_PROJECT = Join-Path $local 'Lucille/Interface'
    $env:LUCILLE_STAGING_ROOT = $local
    if ($Cli) { & (Join-Path $PSScriptRoot 'app/lucille-cli.exe') @CliArguments; $code = $LASTEXITCODE }
    else { $process = Start-Process -FilePath (Join-Path $PSScriptRoot 'app/Lucille.exe') -PassThru -WindowStyle Hidden; $code = 0 }
} finally {
    foreach ($key in $old.Keys) { [Environment]::SetEnvironmentVariable($key, $old[$key], 'Process') }
}
exit $code
