# Experimento: capturar a apresentacao de quadros via ETW e calcular frametime
# por processo, sem tocar em nenhum jogo.
#
# Precisa de PowerShell ELEVADO (Executar como administrador): iniciar uma
# sessao ETW e negado a usuario comum.
#
# Grava uma sessao curta do provedor Microsoft-Windows-DXGI num arquivo
# temporario, le os eventos e escreve o resumo em frametime-etw-resultado.txt,
# ao lado deste script. O arquivo temporario e apagado no final.

param(
    [int]$Segundos = 10
)

$sessao = 'PrumoEtwTeste'
$etl = Join-Path $env:TEMP 'prumo-etw-teste.etl'
$saida = Join-Path $PSScriptRoot 'frametime-etw-resultado.txt'
$linhas = New-Object System.Collections.Generic.List[string]
function Dizer($texto) { $linhas.Add([string]$texto); Write-Host $texto }

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host 'Este teste precisa de PowerShell elevado (Executar como administrador).'
    exit 1
}

Get-ChildItem (Join-Path $env:TEMP 'prumo-etw-teste*.etl') -ErrorAction SilentlyContinue | Remove-Item -Force
cmd /c "logman stop $sessao -ets >nul 2>&1"

cmd /c "logman start $sessao -p Microsoft-Windows-DXGI 0xFFFFFFFFFFFFFFFF 4 -o `"$etl`" -ets >nul 2>&1"
if ($LASTEXITCODE -ne 0) {
    Write-Host "Nao foi possivel iniciar a sessao ETW (codigo $LASTEXITCODE)."
    exit 1
}

Write-Host "Capturando por $Segundos segundos. Deixe algo renderizando (jogo, Unity em Play, video)."
Start-Sleep -Seconds $Segundos
cmd /c "logman stop $sessao -ets >nul 2>&1"

$arquivo = Get-ChildItem (Join-Path $env:TEMP 'prumo-etw-teste*.etl') | Sort-Object Length -Descending | Select-Object -First 1
if (-not $arquivo) {
    Write-Host 'A sessao nao gerou arquivo.'
    exit 1
}

$sw = [Diagnostics.Stopwatch]::StartNew()
$eventos = @(Get-WinEvent -Path $arquivo.FullName -Oldest -MaxEvents 300000 -ErrorAction SilentlyContinue)
$sw.Stop()

Dizer "Data: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
Dizer "Duracao da captura: $Segundos s"
Dizer "Arquivo: $([math]::Round($arquivo.Length / 1MB, 2)) MB  ($([math]::Round($arquivo.Length / 1MB / $Segundos * 3600, 0)) MB por hora nesse ritmo)"
Dizer "Eventos lidos: $($eventos.Count)  ($([math]::Round($eventos.Count / $Segundos, 0)) por segundo)"
Dizer "Tempo de leitura do arquivo: $($sw.ElapsedMilliseconds) ms"
Dizer ''
Dizer 'Eventos por ID (os 10 mais frequentes):'
$eventos | Group-Object Id | Sort-Object Count -Descending | Select-Object -First 10 | ForEach-Object {
    Dizer ("  ID {0,-5} {1,8} eventos" -f $_.Name, $_.Count)
}

# ID 42 = inicio de Present; ID 55 = inicio de PresentMultiplaneOverlay.
foreach ($idQuadro in 42, 55) {
    Dizer ''
    Dizer "Frametime por processo (evento ID $idQuadro):"
    $porProcesso = $eventos | Where-Object { $_.Id -eq $idQuadro } | Group-Object ProcessId
    if (-not $porProcesso) { Dizer '  nenhum evento'; continue }
    foreach ($g in ($porProcesso | Sort-Object Count -Descending | Select-Object -First 8)) {
        $ticks = @($g.Group | ForEach-Object { $_.TimeCreated.Ticks } | Sort-Object)
        if ($ticks.Count -lt 3) { continue }
        $ms = for ($i = 1; $i -lt $ticks.Count; $i++) { ($ticks[$i] - $ticks[$i - 1]) / 10000.0 }
        $ms = @($ms | Sort-Object)
        $duracao = ($ticks[-1] - $ticks[0]) / 10000000.0
        $fps = if ($duracao -gt 0) { ($ticks.Count - 1) / $duracao } else { 0 }
        $p99 = $ms[[math]::Min($ms.Count - 1, [int][math]::Floor($ms.Count * 0.99))]
        $nome = try { (Get-Process -Id $g.Name -ErrorAction Stop).ProcessName } catch { '?' }
        Dizer ("  {0,-22} pid {1,-6} quadros {2,5}  FPS medio {3,6:N1}  frametime medio {4,6:N2} ms  P99 {5,6:N2} ms  max {6,7:N2} ms" -f `
            $nome, $g.Name, $ticks.Count, $fps, (($ms | Measure-Object -Average).Average), $p99, $ms[-1])
    }
}

$linhas | Set-Content -Path $saida -Encoding utf8
Remove-Item $arquivo.FullName -Force -ErrorAction SilentlyContinue
Write-Host ''
Write-Host "Resumo gravado em $saida"
