# Experimento: atribuir a memoria compartilhada usando so API documentada.
#
# Para cada processo, percorre as regioes de memoria (VirtualQueryEx) e pergunta
# ao Windows, pagina por pagina, se ela esta na RAM, se e compartilhada e por
# quantos processos (QueryWorkingSetEx).
#
# Somente leitura. Abre os processos apenas com PROCESS_QUERY_INFORMATION:
# nao le o conteudo da memoria de ninguem.

param(
    # Nome de processo (sem .exe) para restringir o teste. Vazio = todos.
    [string]$Filtro = ''
)

$codigo = @'
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;

public static class WsProbe
{
    [StructLayout(LayoutKind.Sequential)]
    struct MBI
    {
        public IntPtr BaseAddress;
        public IntPtr AllocationBase;
        public uint AllocationProtect;
        public ushort PartitionId;
        public IntPtr RegionSize;
        public uint State;
        public uint Protect;
        public uint Type;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct WSX
    {
        public IntPtr Va;
        public UIntPtr Flags;
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern IntPtr OpenProcess(uint access, bool inherit, int pid);

    [DllImport("kernel32.dll")]
    static extern bool CloseHandle(IntPtr h);

    [DllImport("kernel32.dll")]
    static extern IntPtr VirtualQueryEx(IntPtr h, IntPtr addr, out MBI mbi, IntPtr len);

    [DllImport("psapi.dll", SetLastError = true)]
    static extern bool QueryWorkingSetEx(IntPtr h, [In, Out] WSX[] buf, int cb);

    const uint PROCESS_QUERY_INFORMATION = 0x0400;
    const uint MEM_COMMIT = 0x1000;
    const uint MEM_IMAGE = 0x1000000;
    const uint MEM_MAPPED = 0x40000;
    const int LOTE = 65536;

    public static long PaginasPrivadas, PaginasConsultadas;
    public static long Abertos, Negados, WorkingSetNegado;
    // Paginas compartilhadas presentes na RAM, por contagem de compartilhamento (0..7).
    public static long[] Imagem = new long[8];
    public static long[] Mapeado = new long[8];
    public static long[] Outro = new long[8];

    public static void Run(string filtro)
    {
        WSX[] buf = new WSX[LOTE];
        foreach (Process p in Process.GetProcesses())
        {
            if (p.Id == 0) continue;
            if (!String.IsNullOrEmpty(filtro) && !String.Equals(p.ProcessName, filtro, StringComparison.OrdinalIgnoreCase)) continue;
            IntPtr h = OpenProcess(PROCESS_QUERY_INFORMATION, false, p.Id);
            if (h == IntPtr.Zero)
            {
                Negados++;
                try { WorkingSetNegado += p.WorkingSet64; } catch { }
                continue;
            }
            Abertos++;
            long addr = 0;
            MBI mbi;
            while (addr < 0x7FFFFFFF0000L &&
                   VirtualQueryEx(h, new IntPtr(addr), out mbi, new IntPtr(48)) != IntPtr.Zero)
            {
                long b = mbi.BaseAddress.ToInt64();
                long size = mbi.RegionSize.ToInt64();
                if (size <= 0) break;
                if (mbi.State == MEM_COMMIT)
                {
                    long[] tally = mbi.Type == MEM_IMAGE ? Imagem : (mbi.Type == MEM_MAPPED ? Mapeado : Outro);
                    long pages = size >> 12;
                    long done = 0;
                    while (done < pages)
                    {
                        int n = (int)Math.Min((long)LOTE, pages - done);
                        for (int i = 0; i < n; i++)
                        {
                            buf[i].Va = new IntPtr(b + ((done + i) << 12));
                            buf[i].Flags = UIntPtr.Zero;
                        }
                        if (!QueryWorkingSetEx(h, buf, n * 16)) break;
                        for (int i = 0; i < n; i++)
                        {
                            ulong f = buf[i].Flags.ToUInt64();
                            if ((f & 1) == 0) continue;               // fora da RAM
                            if ((f & 0x8000) != 0) tally[(f >> 1) & 7]++; // compartilhada
                            else PaginasPrivadas++;
                        }
                        done += n;
                        PaginasConsultadas += n;
                    }
                }
                addr = b + size;
            }
            CloseHandle(h);
        }
    }
}
'@

Add-Type -TypeDefinition $codigo

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$sw = [Diagnostics.Stopwatch]::StartNew()
[WsProbe]::Run($Filtro)
$sw.Stop()

function ToGb($paginas) { [math]::Round($paginas * 4096 / 1GB, 2) }

function Resumo($nome, $t) {
    $bruto = ($t | Measure-Object -Sum).Sum
    # Divide cada pagina pelo numero de processos que a compartilham.
    # O contador do Windows satura em 7: paginas com 7 ou mais donos ficam superestimadas.
    $unico = 0.0
    for ($c = 0; $c -lt 8; $c++) { $unico += $t[$c] / [math]::Max($c, 1) }
    [pscustomobject]@{
        Tipo               = $nome
        BrutoGB            = ToGb $bruto
        DivididoGB         = ToGb $unico
        Saturadas7ouMaisGB = ToGb $t[7]
    }
}

"Administrador: $admin"
"Tempo: $($sw.ElapsedMilliseconds) ms"
"Processos abertos: $([WsProbe]::Abertos)   negados: $([WsProbe]::Negados)   working set dos negados: $([math]::Round([WsProbe]::WorkingSetNegado / 1GB, 2)) GB"
"Paginas consultadas: $([WsProbe]::PaginasConsultadas)"
"Privado na RAM: $(ToGb ([WsProbe]::PaginasPrivadas)) GB"
""
@(
    Resumo 'Imagem (exe/dll)' ([WsProbe]::Imagem)
    Resumo 'Arquivo mapeado' ([WsProbe]::Mapeado)
    Resumo 'Outro compartilhado' ([WsProbe]::Outro)
) | Format-Table -AutoSize

"Paginas compartilhadas por contagem de donos (0 a 7):"
"  Imagem : $([WsProbe]::Imagem -join ' ')"
"  Mapeado: $([WsProbe]::Mapeado -join ' ')"
