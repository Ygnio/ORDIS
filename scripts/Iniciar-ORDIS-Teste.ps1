$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root 'app\ORDIS.exe'
$dataRoot = Join-Path $env:LOCALAPPDATA 'Lucille\ORDIS-Teste'

if (-not (Test-Path -LiteralPath $exe)) {
    throw "ORDIS.exe não encontrado em $exe. Extraia o ZIP por completo antes de iniciar."
}

# Cria apenas a estrutura ausente. Preferências, prompt local e dados existentes
# nunca são substituídos por arquivos do pacote.
$files = @(
    @{ From = 'config\paths.json'; To = 'config\paths.json' },
    @{ From = 'app\ui\ordis.png'; To = 'app\ui\ordis.png' },
    @{ From = 'agente\workspace\AGENTS.md'; To = 'agente\workspace\AGENTS.md' }
)
foreach ($relative in @('config', 'app\ui', 'agente\workspace', 'scripts', 'runtime')) {
    New-Item -ItemType Directory -Force -Path (Join-Path $dataRoot $relative) | Out-Null
}
foreach ($file in $files) {
    $destination = Join-Path $dataRoot $file.To
    if (-not (Test-Path -LiteralPath $destination)) {
        Copy-Item -LiteralPath (Join-Path $root $file.From) -Destination $destination
    }
}
foreach ($name in @('serve_inference.py', 'service_shutdown.py', 'agent_web_search.py', 'ordis_token_monitor.py', 'token_counter_paths.py')) {
    $destination = Join-Path $dataRoot "scripts\$name"
    if (-not (Test-Path -LiteralPath $destination)) {
        Copy-Item -LiteralPath (Join-Path $root "scripts\$name") -Destination $destination
    }
}

$previousProject = $env:ORDIS_PROJECT
try {
    $env:ORDIS_PROJECT = $dataRoot
    Start-Process -FilePath $exe -WorkingDirectory (Split-Path -Parent $exe)
}
finally {
    if ($null -eq $previousProject) { Remove-Item Env:ORDIS_PROJECT -ErrorAction SilentlyContinue }
    else { $env:ORDIS_PROJECT = $previousProject }
}
