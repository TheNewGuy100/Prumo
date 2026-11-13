# Experimento: comparar as diferentes definicoes de "uso" que o Windows oferece
# para CPU, GPU e VRAM. Somente leitura, exceto por uma carga artificial de CPU
# de poucos segundos (8 processos em laco), encerrada ao final.

$ErrorActionPreference = 'Stop'
$nucleos = [Environment]::ProcessorCount

# ---------- CPU ----------
$pi = 'Processor Information'
$cTempo = New-Object Diagnostics.PerformanceCounter($pi, '% Processor Time', '_Total')
$cUtil  = New-Object Diagnostics.PerformanceCounter($pi, '% Processor Utility', '_Total')
$cPerf  = New-Object Diagnostics.PerformanceCounter($pi, '% Processor Performance', '_Total')
$cProc  = New-Object Diagnostics.PerformanceCounter('Process', '% Processor Time', '_Total')
$cIdle  = New-Object Diagnostics.PerformanceCounter('Process', '% Processor Time', 'Idle')
$todos = @($cTempo, $cUtil, $cPerf, $cProc, $cIdle)

function Amostrar($rotulo, $n) {
    foreach ($c in $todos) { [void]$c.NextValue() }
    for ($i = 0; $i -lt $n; $i++) {
        Start-Sleep -Milliseconds 1000
        $somaProc = ($cProc.NextValue() - $cIdle.NextValue()) / $nucleos
        [pscustomobject]@{
            Cenario        = $rotulo
            Tempo          = [math]::Round($cTempo.NextValue(), 1)
            Utility        = [math]::Round($cUtil.NextValue(), 1)
            Desempenho     = [math]::Round($cPerf.NextValue(), 1)
            SomaProcessos  = [math]::Round($somaProc, 1)
        }
    }
}

"=== CPU ($nucleos processadores logicos) ==="
$linhas = @(Amostrar 'como esta' 3)
$carga = @()
try {
    $carga = 1..8 | ForEach-Object {
        Start-Process powershell -ArgumentList '-NoProfile', '-Command', 'while($true){}' -WindowStyle Hidden -PassThru
    }
    Start-Sleep -Milliseconds 1500
    $linhas += @(Amostrar '+8 threads' 4)
}
finally {
    $carga | ForEach-Object { try { Stop-Process -Id $_.Id -Force } catch { } }
}
$linhas | Format-Table -AutoSize

# ---------- GPU: uso por motor ----------
"=== GPU: uso por motor ==="
$cat = New-Object Diagnostics.PerformanceCounterCategory('GPU Engine')
$u = 'Utilization Percentage'
$a = $cat.ReadCategory()
Start-Sleep -Milliseconds 1000
$b = $cat.ReadCategory()

$amostras = foreach ($k in $b[$u].Keys) {
    if (-not $a[$u].Contains($k)) { continue }
    $v = [Diagnostics.CounterSample]::Calculate($a[$u][$k].Sample, $b[$u][$k].Sample)
    if ($k -match '^pid_(\d+)_luid_(\w+?_\w+?)_phys_(\d+)_eng_(\d+)_engtype_(.+)$') {
        [pscustomobject]@{ Pid = [int]$Matches[1]; Luid = $Matches[2]; Motor = [int]$Matches[4]; Tipo = $Matches[5]; Uso = $v }
    }
}

$porMotor = $amostras | Group-Object Luid, Motor, Tipo | ForEach-Object {
    [pscustomobject]@{
        Adaptador = $_.Group[0].Luid
        Motor     = $_.Group[0].Motor
        Tipo      = $_.Group[0].Tipo
        UsoPct    = [math]::Round(($_.Group | Measure-Object Uso -Sum).Sum, 1)
    }
}
$porMotor | Where-Object UsoPct -gt 0.05 | Sort-Object UsoPct -Descending | Format-Table -AutoSize
"Motores existentes: $(@($porMotor).Count)   instancias (processo x motor): $(@($amostras).Count)"
$porAdaptador = $porMotor | Group-Object Adaptador | ForEach-Object {
    [pscustomobject]@{
        Adaptador       = $_.Name
        MaiorMotorPct   = ($_.Group | Measure-Object UsoPct -Maximum).Maximum
        SomaMotoresPct  = [math]::Round(($_.Group | Measure-Object UsoPct -Sum).Sum, 1)
    }
}
$porAdaptador | Format-Table -AutoSize

"Processos com mais uso de GPU (soma dos motores):"
$amostras | Group-Object Pid | ForEach-Object {
    $nome = try { (Get-Process -Id $_.Name).ProcessName } catch { '?' }
    [pscustomobject]@{ Processo = $nome; Pid = $_.Name; UsoPct = [math]::Round(($_.Group | Measure-Object Uso -Sum).Sum, 1) }
} | Sort-Object UsoPct -Descending | Select-Object -First 5 | Format-Table -AutoSize

# ---------- VRAM ----------
"=== VRAM ==="
function LerBytes($categoria, $contador) {
    $c = New-Object Diagnostics.PerformanceCounterCategory($categoria)
    $d = $c.ReadCategory()
    foreach ($k in $d[$contador].Keys) {
        [pscustomobject]@{ Instancia = $k; Bytes = $d[$contador][$k].RawValue }
    }
}
function Gb($v) { [math]::Round($v / 1GB, 2) }

$adDed = LerBytes 'GPU Adapter Memory' 'Dedicated Usage'
$adSha = LerBytes 'GPU Adapter Memory' 'Shared Usage'
$adCom = LerBytes 'GPU Adapter Memory' 'Total Committed'
$prDed = LerBytes 'GPU Process Memory' 'Dedicated Usage'
$prSha = LerBytes 'GPU Process Memory' 'Shared Usage'

$vram = foreach ($ad in $adDed) {
    $luid = $ad.Instancia -replace '_phys_\d+$', ''
    $sumDed = ($prDed | Where-Object { $_.Instancia -like "*$luid*" } | Measure-Object Bytes -Sum).Sum
    $sumSha = ($prSha | Where-Object { $_.Instancia -like "*$luid*" } | Measure-Object Bytes -Sum).Sum
    [pscustomobject]@{
        Adaptador             = $luid
        DedicadaAdaptadorGB   = Gb $ad.Bytes
        DedicadaSomaProcGB    = Gb $sumDed
        CompartAdaptadorGB    = Gb (($adSha | Where-Object Instancia -eq $ad.Instancia).Bytes)
        CompartSomaProcGB     = Gb $sumSha
        TotalComprometidoGB   = Gb (($adCom | Where-Object Instancia -eq $ad.Instancia).Bytes)
    }
}
$vram | Format-List

"Processos com mais VRAM dedicada:"
$prDed | Sort-Object Bytes -Descending | Select-Object -First 6 | ForEach-Object {
    $id = if ($_.Instancia -match '^pid_(\d+)_') { $Matches[1] } else { 0 }
    $nome = try { (Get-Process -Id $id).ProcessName } catch { '?' }
    [pscustomobject]@{ Processo = $nome; Pid = $id; DedicadaMB = [math]::Round($_.Bytes / 1MB) }
} | Format-Table -AutoSize

# ---------- Comparacao com o driver da NVIDIA ----------
"=== nvidia-smi ==="
$smi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
if ($smi) {
    & $smi.Source --query-gpu=name,utilization.gpu,utilization.memory,memory.used,memory.total --format=csv
} else {
    "nvidia-smi nao encontrado"
}
