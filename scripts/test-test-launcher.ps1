$ErrorActionPreference = 'Stop'
$testBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$fixtureRoot = Join-Path $testBase ('ordis-launcher-test-' + [guid]::NewGuid().ToString('N'))
$package = Join-Path $fixtureRoot 'package'
$oldLocalAppData = $env:LOCALAPPDATA
$oldProject = $env:ORDIS_PROJECT
$launches = [System.Collections.Generic.List[object]]::new()
$global:ordisLauncherTestLaunches = $launches

# Substitui somente o lançamento neste processo de teste; nenhum EXE é executado.
function Start-Process {
    param([string]$FilePath, [string]$WorkingDirectory)
    $global:ordisLauncherTestLaunches.Add(@{ FilePath = $FilePath; Project = $env:ORDIS_PROJECT })
}

try {
    $env:LOCALAPPDATA = Join-Path $fixtureRoot 'synthetic-localappdata'
    $env:ORDIS_PROJECT = 'synthetic-previous-project'
    $files = @('app\ORDIS.exe', 'app\ui\ordis.png', 'config\paths.json', 'agente\workspace\AGENTS.md',
        'scripts\serve_inference.py', 'scripts\service_shutdown.py', 'scripts\agent_web_search.py',
        'scripts\ordis_token_monitor.py', 'scripts\token_counter_paths.py')
    foreach ($name in $files) {
        $path = Join-Path $package $name
        New-Item -ItemType Directory -Force (Split-Path -Parent $path) | Out-Null
        Set-Content -LiteralPath $path -Value 'synthetic-public-fixture'
    }
    $launcher = Join-Path $package 'Iniciar-ORDIS-Teste.ps1'
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Iniciar-ORDIS-Teste.ps1') -Destination $launcher
    & $launcher
    $dataRoot = Join-Path $env:LOCALAPPDATA 'Lucille\ORDIS-Teste'
    if ($launches.Count -ne 1 -or $launches[0].Project -ne $dataRoot) { throw 'Launcher selected the wrong data directory.' }
    if ($env:ORDIS_PROJECT -ne 'synthetic-previous-project') { throw 'Launcher did not restore ORDIS_PROJECT.' }
    $privateSettings = Join-Path $dataRoot 'runtime\settings.json'
    $history = Join-Path $dataRoot 'runtime\synthetic-history.json'
    $config = Join-Path $dataRoot 'config\paths.json'
    Set-Content -LiteralPath $privateSettings -Value 'synthetic-existing-preferences'
    Set-Content -LiteralPath $history -Value 'synthetic-existing-history'
    Set-Content -LiteralPath $config -Value 'synthetic-existing-config'
    & $launcher
    if ((Get-Content -Raw -LiteralPath $privateSettings).Trim() -ne 'synthetic-existing-preferences') { throw 'Preferences were replaced.' }
    if ((Get-Content -Raw -LiteralPath $history).Trim() -ne 'synthetic-existing-history') { throw 'History was replaced.' }
    if ((Get-Content -Raw -LiteralPath $config).Trim() -ne 'synthetic-existing-config') { throw 'Existing configuration was replaced.' }
    # Compatibilidade com a localização original em scripts/.
    $nestedLauncher = Join-Path $package 'scripts\Iniciar-ORDIS-Teste.ps1'
    Copy-Item -LiteralPath $launcher -Destination $nestedLauncher
    & $nestedLauncher
    if ($launches.Count -ne 3 -or $launches[2].FilePath -ne (Join-Path $package 'app\ORDIS.exe')) { throw 'Nested launcher failed.' }
    Write-Host 'Launcher tests passed: package root, legacy script path, isolated data, preserved history/preferences/configuration.'
}
finally {
    Remove-Variable -Name ordisLauncherTestLaunches -Scope Global -ErrorAction SilentlyContinue
    $env:LOCALAPPDATA = $oldLocalAppData
    $env:ORDIS_PROJECT = $oldProject
    $resolvedFixture = [System.IO.Path]::GetFullPath($fixtureRoot)
    if (-not $resolvedFixture.StartsWith($testBase, [StringComparison]::OrdinalIgnoreCase) -or
        (Split-Path -Leaf $resolvedFixture) -notmatch '^ordis-launcher-test-[a-f0-9]{32}$') {
        throw 'Unsafe fixture cleanup target.'
    }
    if (Test-Path -LiteralPath $resolvedFixture) { Remove-Item -LiteralPath $resolvedFixture -Recurse -Force }
}
