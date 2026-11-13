//! Coletor: lê contadores de desempenho documentados do Windows (PDH), sem
//! privilégio. As definições de cada número estão em GLOSSARIO.md.

use std::collections::HashMap;
use std::ptr;
use std::time::Instant;

// ---------------------------------------------------------------------------
// FFI mínima: pdh.dll e kernel32.dll
// ---------------------------------------------------------------------------

type Handle = isize;

#[repr(C)]
#[derive(Clone, Copy)]
union PdhValor {
    duplo: f64,
    grande: i64,
    ponteiro: *const u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PdhValorFormatado {
    status: u32,
    valor: PdhValor,
}

#[repr(C)]
struct PdhItem {
    nome: *const u16,
    valor: PdhValorFormatado,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FileTime {
    baixo: u32,
    alto: u32,
}

#[repr(C)]
#[derive(Default)]
struct MemoryStatusEx {
    tamanho: u32,
    carga: u32,
    total_fisica: u64,
    disponivel_fisica: u64,
    total_paginacao: u64,
    disponivel_paginacao: u64,
    total_virtual: u64,
    disponivel_virtual: u64,
    disponivel_estendida: u64,
}

#[repr(C)]
#[derive(Default)]
struct ProcessMemoryCounters {
    cb: u32,
    faltas_de_pagina: u32,
    pico_working_set: usize,
    working_set: usize,
    quota_pico_paginada: usize,
    quota_paginada: usize,
    quota_pico_nao_paginada: usize,
    quota_nao_paginada: usize,
    uso_pagefile: usize,
    pico_pagefile: usize,
}

#[link(name = "pdh")]
extern "system" {
    fn PdhOpenQueryW(fonte: *const u16, usuario: usize, consulta: *mut Handle) -> u32;
    fn PdhAddEnglishCounterW(
        consulta: Handle,
        caminho: *const u16,
        usuario: usize,
        contador: *mut Handle,
    ) -> u32;
    fn PdhCollectQueryData(consulta: Handle) -> u32;
    fn PdhGetFormattedCounterArrayW(
        contador: Handle,
        formato: u32,
        tamanho: *mut u32,
        quantidade: *mut u32,
        buffer: *mut PdhItem,
    ) -> u32;
    fn PdhGetFormattedCounterValue(
        contador: Handle,
        formato: u32,
        tipo: *mut u32,
        valor: *mut PdhValorFormatado,
    ) -> u32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn GetProcessTimes(
        processo: Handle,
        criacao: *mut FileTime,
        saida: *mut FileTime,
        kernel: *mut FileTime,
        usuario: *mut FileTime,
    ) -> i32;
    fn GlobalMemoryStatusEx(estado: *mut MemoryStatusEx) -> i32;
    fn K32GetProcessMemoryInfo(processo: Handle, contadores: *mut ProcessMemoryCounters, cb: u32) -> i32;
    fn GetLogicalProcessorInformationEx(relacao: u32, buffer: *mut u8, tamanho: *mut u32) -> i32;
    fn OpenProcess(acesso: u32, herdar: i32, pid: u32) -> Handle;
    fn QueryFullProcessImageNameW(processo: Handle, formato: u32, nome: *mut u16, tamanho: *mut u32) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}

#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(
        processo: Handle,
        classe: u32,
        informacao: *mut u8,
        tamanho: u32,
        tamanho_devolvido: *mut u32,
    ) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn EnumWindows(funcao: extern "system" fn(Handle, isize) -> i32, parametro: isize) -> i32;
    fn IsWindowVisible(janela: Handle) -> i32;
    fn GetWindowTextW(janela: Handle, texto: *mut u16, maximo: i32) -> i32;
    fn GetWindowThreadProcessId(janela: Handle, pid: *mut u32) -> u32;
}

#[link(name = "advapi32")]
extern "system" {
    fn OpenSCManagerW(maquina: *const u16, banco: *const u16, acesso: u32) -> Handle;
    fn EnumServicesStatusExW(
        gerenciador: Handle,
        nivel: u32,
        tipo: u32,
        estado: u32,
        buffer: *mut u8,
        tamanho: u32,
        necessario: *mut u32,
        devolvidos: *mut u32,
        retomada: *mut u32,
        grupo: *const u16,
    ) -> i32;
    fn OpenServiceW(gerenciador: Handle, nome: *const u16, acesso: u32) -> Handle;
    fn QueryServiceConfigW(servico: Handle, buffer: *mut u8, tamanho: u32, necessario: *mut u32) -> i32;
    fn CloseServiceHandle(handle: Handle) -> i32;
}

const SC_MANAGER_ENUMERATE_SERVICE: u32 = 0x0004;
const SERVICE_QUERY_CONFIG: u32 = 0x0001;
const SERVICE_WIN32: u32 = 0x0000_0030;
const SERVICE_ACTIVE: u32 = 0x0000_0001;
/// Tamanho de um registro ENUM_SERVICE_STATUS_PROCESSW em 64 bits.
const TAMANHO_DO_REGISTRO_DE_SERVICO: usize = 56;

/// Classe de informação que devolve a linha de comando de um processo.
const PROCESS_COMMAND_LINE_INFORMATION: u32 = 60;
/// Linhas de comando maiores que isto são cortadas.
const MAXIMO_DA_LINHA_DE_COMANDO: usize = 2000;

const PDH_FMT_DOUBLE: u32 = 0x0000_0200;
const PDH_FMT_NOCAP100: u32 = 0x0000_8000;
const PDH_MORE_DATA: u32 = 0x8000_07D2;
const RELATION_PROCESSOR_CORE: u32 = 0;
/// Só consulta de informações básicas; não dá acesso à memória do processo.
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

fn largo(texto: &str) -> Vec<u16> {
    texto.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------------------
// Consulta PDH
// ---------------------------------------------------------------------------

struct Consulta {
    handle: Handle,
}

/// Contador que pode não existir na máquina (por exemplo, sem GPU).
#[derive(Clone, Copy)]
struct Contador(Option<Handle>);

impl Consulta {
    fn nova() -> Consulta {
        let mut handle: Handle = 0;
        let erro = unsafe { PdhOpenQueryW(ptr::null(), 0, &mut handle) };
        if erro != 0 {
            eprintln!("Não foi possível abrir a consulta de desempenho (erro {erro:#x}).");
            std::process::exit(1);
        }
        Consulta { handle }
    }

    fn contador(&self, caminho: &str) -> Contador {
        let mut contador: Handle = 0;
        let caminho_largo = largo(caminho);
        let erro = unsafe { PdhAddEnglishCounterW(self.handle, caminho_largo.as_ptr(), 0, &mut contador) };
        if erro != 0 {
            eprintln!("Contador indisponível: {caminho} (erro {erro:#x})");
            return Contador(None);
        }
        Contador(Some(contador))
    }

    fn coletar(&self) {
        unsafe { PdhCollectQueryData(self.handle) };
    }
}

impl Contador {
    /// Valor único (contador sem curinga).
    fn valor(&self) -> Option<f64> {
        let handle = self.0?;
        let mut saida = PdhValorFormatado { status: 0, valor: PdhValor { grande: 0 } };
        let erro = unsafe {
            PdhGetFormattedCounterValue(handle, PDH_FMT_DOUBLE | PDH_FMT_NOCAP100, ptr::null_mut(), &mut saida)
        };
        if erro != 0 || saida.status > 1 {
            return None;
        }
        Some(unsafe { saida.valor.duplo })
    }

    /// Todas as instâncias de um contador com curinga, como (nome, valor).
    fn instancias(&self) -> Vec<(String, f64)> {
        let Some(handle) = self.0 else { return Vec::new() };
        let formato = PDH_FMT_DOUBLE | PDH_FMT_NOCAP100;
        let mut tamanho: u32 = 0;
        let mut quantidade: u32 = 0;
        let erro = unsafe {
            PdhGetFormattedCounterArrayW(handle, formato, &mut tamanho, &mut quantidade, ptr::null_mut())
        };
        if erro != PDH_MORE_DATA || tamanho == 0 {
            return Vec::new();
        }
        // Buffer em u64 para garantir alinhamento de 8 bytes; folga para instâncias novas.
        let mut tamanho = tamanho + 4096;
        let mut buffer: Vec<u64> = vec![0; (tamanho as usize + 7) / 8];
        let erro = unsafe {
            PdhGetFormattedCounterArrayW(
                handle,
                formato,
                &mut tamanho,
                &mut quantidade,
                buffer.as_mut_ptr() as *mut PdhItem,
            )
        };
        if erro != 0 {
            return Vec::new();
        }
        let itens = buffer.as_ptr() as *const PdhItem;
        let mut saida = Vec::with_capacity(quantidade as usize);
        for i in 0..quantidade as usize {
            let item = unsafe { &*itens.add(i) };
            if item.valor.status > 1 || item.nome.is_null() {
                continue;
            }
            let mut comprimento = 0usize;
            while unsafe { *item.nome.add(comprimento) } != 0 {
                comprimento += 1;
            }
            let nome = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(item.nome, comprimento) });
            saida.push((nome, unsafe { item.valor.valor.duplo }));
        }
        saida
    }
}

// ---------------------------------------------------------------------------
// Leituras diretas do sistema
// ---------------------------------------------------------------------------

fn tempo_de_cpu_proprio() -> u64 {
    let (mut criacao, mut saida, mut kernel, mut usuario) =
        (FileTime::default(), FileTime::default(), FileTime::default(), FileTime::default());
    unsafe { GetProcessTimes(GetCurrentProcess(), &mut criacao, &mut saida, &mut kernel, &mut usuario) };
    let juntar = |f: FileTime| ((f.alto as u64) << 32) | f.baixo as u64;
    juntar(kernel) + juntar(usuario) // unidades de 100 ns
}

fn working_set_proprio() -> u64 {
    let mut contadores = ProcessMemoryCounters::default();
    contadores.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut contadores, contadores.cb) };
    contadores.working_set as u64
}

/// Mede o custo do processo atual entre uma chamada e a seguinte.
pub struct Medidor {
    cpu_antes: u64,
    relogio: Instant,
    logicos: f64,
}

impl Medidor {
    pub fn novo() -> Medidor {
        Medidor {
            cpu_antes: tempo_de_cpu_proprio(),
            relogio: Instant::now(),
            logicos: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64,
        }
    }

    /// (% de CPU da máquina inteira, MB de working set).
    pub fn medir(&mut self) -> (f64, f64) {
        let agora = tempo_de_cpu_proprio();
        let decorrido = self.relogio.elapsed().as_secs_f64().max(0.001);
        let cpu = (agora - self.cpu_antes) as f64 / 1e7 / decorrido / self.logicos * 100.0;
        self.cpu_antes = agora;
        self.relogio = Instant::now();
        (cpu, working_set_proprio() as f64 / (1024.0 * 1024.0))
    }
}

fn memoria_fisica_total() -> u64 {
    let mut estado = MemoryStatusEx::default();
    estado.tamanho = std::mem::size_of::<MemoryStatusEx>() as u32;
    unsafe { GlobalMemoryStatusEx(&mut estado) };
    estado.total_fisica
}

/// Linha de comando de um processo já aberto; vazia se o Windows não informar.
fn linha_de_comando(processo: Handle) -> String {
    let mut necessario: u32 = 0;
    unsafe {
        NtQueryInformationProcess(processo, PROCESS_COMMAND_LINE_INFORMATION, ptr::null_mut(), 0, &mut necessario)
    };
    if necessario < 16 || necessario > 1024 * 1024 {
        return String::new();
    }
    // O buffer recebe um UNICODE_STRING (tamanho em bytes, ponteiro) seguido do texto.
    let mut buffer: Vec<u64> = vec![0; (necessario as usize + 7) / 8 + 1];
    let status = unsafe {
        NtQueryInformationProcess(
            processo,
            PROCESS_COMMAND_LINE_INFORMATION,
            buffer.as_mut_ptr() as *mut u8,
            (buffer.len() * 8) as u32,
            &mut necessario,
        )
    };
    if status < 0 {
        return String::new();
    }
    let bytes = (buffer[0] & 0xFFFF) as usize;
    let texto = buffer[1] as *const u16;
    let inicio = buffer.as_ptr() as usize;
    let fim = inicio + buffer.len() * 8;
    // O ponteiro tem que apontar para dentro do nosso próprio buffer.
    if texto.is_null() || (texto as usize) < inicio || (texto as usize) + bytes > fim {
        return String::new();
    }
    let unidades = unsafe { std::slice::from_raw_parts(texto, bytes / 2) };
    let mut linha = String::from_utf16_lossy(unidades);
    if linha.chars().count() > MAXIMO_DA_LINHA_DE_COMANDO {
        linha = linha.chars().take(MAXIMO_DA_LINHA_DE_COMANDO).collect::<String>() + "…";
    }
    linha
}

/// (caminho do executável, linha de comando) de um processo. Cada um fica
/// vazio se o Windows não deixar consultar. Não lê a memória do processo.
pub fn identidade_do_processo(pid: u32) -> (String, String) {
    let processo = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if processo == 0 {
        return (String::new(), String::new());
    }
    let mut buffer = vec![0u16; 1024];
    let mut tamanho = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(processo, 0, buffer.as_mut_ptr(), &mut tamanho) };
    let caminho = if ok != 0 { String::from_utf16_lossy(&buffer[..tamanho as usize]) } else { String::new() };
    let comando = linha_de_comando(processo);
    unsafe { CloseHandle(processo) };
    (caminho, comando)
}

extern "system" fn recolher_janela(janela: Handle, parametro: isize) -> i32 {
    let titulos = unsafe { &mut *(parametro as *mut HashMap<u32, Vec<String>>) };
    if unsafe { IsWindowVisible(janela) } == 0 {
        return 1;
    }
    let mut texto = [0u16; 256];
    let n = unsafe { GetWindowTextW(janela, texto.as_mut_ptr(), texto.len() as i32) };
    if n <= 0 {
        return 1;
    }
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(janela, &mut pid) };
    let lista = titulos.entry(pid).or_default();
    let titulo = String::from_utf16_lossy(&texto[..n as usize]);
    if lista.len() < 4 && !lista.contains(&titulo) {
        lista.push(titulo);
    }
    1 // continua a enumeração
}

/// Texto largo terminado em zero -> String. Ponteiro nulo vira texto vazio.
unsafe fn texto_largo(ponteiro: *const u16) -> String {
    if ponteiro.is_null() {
        return String::new();
    }
    let mut n = 0usize;
    while *ponteiro.add(n) != 0 {
        n += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(ponteiro, n))
}

/// Configuração de um serviço: (tipo de início, conta, linha do binário).
fn configuracao_do_servico(gerenciador: Handle, nome: &str) -> (String, String, String) {
    let vazio = (String::new(), String::new(), String::new());
    let nome_largo = largo(nome);
    let servico = unsafe { OpenServiceW(gerenciador, nome_largo.as_ptr(), SERVICE_QUERY_CONFIG) };
    if servico == 0 {
        return vazio;
    }
    let mut buffer: Vec<u64> = vec![0; 1024]; // 8 KB, o máximo que a API devolve
    let mut necessario: u32 = 0;
    let ok = unsafe {
        QueryServiceConfigW(servico, buffer.as_mut_ptr() as *mut u8, (buffer.len() * 8) as u32, &mut necessario)
    };
    unsafe { CloseServiceHandle(servico) };
    if ok == 0 {
        return vazio;
    }
    // QUERY_SERVICE_CONFIGW em 64 bits: tipo de início no byte 4, binário no 16, conta no 48.
    let inicio = match (buffer[0] >> 32) as u32 {
        0 | 1 => "na partida do sistema",
        2 => "automático",
        3 => "manual",
        4 => "desativado",
        _ => "desconhecido",
    };
    let binario = unsafe { texto_largo(buffer[2] as *const u16) };
    let conta = unsafe { texto_largo(buffer[6] as *const u16) };
    (inicio.to_string(), conta, binario)
}

/// Caminho do executável dentro da linha do binário de um serviço.
fn executavel_do_binario(binario: &str) -> String {
    let binario = binario.trim();
    if let Some(resto) = binario.strip_prefix('"') {
        return resto.split('"').next().unwrap_or("").to_string();
    }
    match binario.to_lowercase().find(".exe") {
        Some(posicao) => binario[..posicao + 4].to_string(),
        None => binario.to_string(),
    }
}

/// Títulos das janelas visíveis, por processo.
fn titulos_das_janelas() -> HashMap<u32, Vec<String>> {
    let mut titulos: HashMap<u32, Vec<String>> = HashMap::new();
    unsafe { EnumWindows(recolher_janela, &mut titulos as *mut _ as isize) };
    titulos
}

/// Núcleos físicos, cada um com os índices dos seus processadores lógicos.
/// Considera só o primeiro grupo de processadores (até 64 lógicos).
fn nucleos_fisicos() -> Vec<Vec<usize>> {
    let mut tamanho: u32 = 0;
    unsafe { GetLogicalProcessorInformationEx(RELATION_PROCESSOR_CORE, ptr::null_mut(), &mut tamanho) };
    if tamanho == 0 {
        return Vec::new();
    }
    let mut buffer: Vec<u64> = vec![0; (tamanho as usize + 7) / 8];
    let ok = unsafe {
        GetLogicalProcessorInformationEx(RELATION_PROCESSOR_CORE, buffer.as_mut_ptr() as *mut u8, &mut tamanho)
    };
    if ok == 0 {
        return Vec::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr() as *const u8, tamanho as usize) };
    let mut nucleos = Vec::new();
    let mut o = 0usize;
    // Registro: Relationship (u32), Size (u32), PROCESSOR_RELATIONSHIP com a
    // primeira máscara de afinidade no deslocamento 32.
    while o + 40 <= bytes.len() {
        let tamanho_registro = u32::from_le_bytes(bytes[o + 4..o + 8].try_into().unwrap()) as usize;
        if tamanho_registro == 0 {
            break;
        }
        let mascara = u64::from_le_bytes(bytes[o + 32..o + 40].try_into().unwrap());
        let logicos: Vec<usize> = (0..64).filter(|bit| mascara & (1u64 << bit) != 0).collect();
        if !logicos.is_empty() {
            nucleos.push(logicos);
        }
        o += tamanho_registro;
    }
    nucleos.sort_by_key(|logicos| logicos[0]);
    nucleos
}

// ---------------------------------------------------------------------------
// Amostra
// ---------------------------------------------------------------------------

/// Um processo dentro de um grupo.
#[derive(Default, Clone)]
pub struct Membro {
    pub pid: u32,
    /// % do tempo de toda a máquina (0 a 100).
    pub cpu: f64,
    pub privada: f64,
    pub working_set: f64,
    pub vram: f64,
    /// Bytes por segundo de entrada e saída: disco, rede e dispositivos.
    pub es_leitura: f64,
    pub es_escrita: f64,
    /// Caminho do executável; vazio se o Windows não deixou consultar.
    pub caminho: String,
    /// Linha de comando com que o processo foi iniciado; pode estar vazia.
    pub comando: String,
    /// Títulos das janelas visíveis do processo.
    pub titulos: Vec<String>,
    /// Serviços do Windows que rodam dentro deste processo.
    pub servicos: Vec<Servico>,
}

/// Um serviço do Windows em execução.
#[derive(Default, Clone)]
pub struct Servico {
    /// Nome interno, o que se usa para parar ou iniciar.
    pub nome: String,
    /// Nome de exibição.
    pub exibicao: String,
    /// "automático", "manual", "desativado" ou "na partida do sistema".
    pub inicio: String,
    /// Conta em que roda, por exemplo "LocalSystem".
    pub conta: String,
}

impl Membro {
    /// Se algum texto que identifica o processo contém o filtro (já em minúsculas).
    pub fn contem(&self, filtro: &str) -> bool {
        self.caminho.to_lowercase().contains(filtro)
            || self.comando.to_lowercase().contains(filtro)
            || self.titulos.iter().any(|t| t.to_lowercase().contains(filtro))
            || self
                .servicos
                .iter()
                .any(|s| s.nome.to_lowercase().contains(filtro) || s.exibicao.to_lowercase().contains(filtro))
    }
}

/// Processos de mesmo nome, somados.
pub struct GrupoDeProcessos {
    pub nome: String,
    pub membros: Vec<Membro>,
    /// % do tempo de toda a máquina (0 a 100).
    pub cpu: f64,
    /// Bytes na RAM que só este grupo usa.
    pub privada: f64,
    /// Bytes de working set: privada mais compartilhado. Não é somável.
    pub working_set: f64,
    /// Motor de GPU mais usado pelo grupo e o uso em %.
    pub gpu: Option<(String, f64)>,
    /// Bytes de VRAM dedicada.
    pub vram: f64,
    pub es_leitura: f64,
    pub es_escrita: f64,
}

impl GrupoDeProcessos {
    pub fn processos(&self) -> usize {
        self.membros.len()
    }

    pub fn pids(&self) -> Vec<u32> {
        self.membros.iter().map(|m| m.pid).collect()
    }

    pub fn es(&self) -> f64 {
        self.es_leitura + self.es_escrita
    }
}

/// Um disco físico.
pub struct Disco {
    /// Como o Windows nomeia: número e letras, por exemplo "0 C:".
    pub nome: String,
    /// % do tempo em que o disco estava atendendo pedidos.
    pub atividade: f64,
    /// Bytes por segundo.
    pub leitura: f64,
    pub escrita: f64,
}

/// Uma interface de rede.
pub struct Rede {
    pub nome: String,
    /// Bytes por segundo.
    pub recebido: f64,
    pub enviado: f64,
}

pub struct Amostra {
    /// Segundos cobertos por esta amostra.
    pub decorrido: f64,
    pub custo_cpu: f64,
    pub custo_mem_mb: f64,
    pub ms_coleta: f64,

    pub cpu_total: f64,
    pub turbo: Option<f64>,
    /// Uso por processador lógico, pelo índice.
    pub por_logico: Vec<f64>,
    /// Soma da CPU dos processos vistos.
    pub cpu_processos: f64,

    pub total_ram: f64,
    pub em_uso: f64,
    pub programas: f64,
    pub windows: f64,
    /// Calculado por diferença.
    pub compartilhado_e_outros: f64,
    pub cache: f64,
    pub modificada: f64,
    pub livre: f64,

    /// Motor mais ocupado do sistema e o uso em %.
    pub gpu: Option<(String, f64)>,
    pub vram_dedicada: Option<f64>,
    pub vram_processos: f64,

    pub discos: Vec<Disco>,
    pub redes: Vec<Rede>,

    pub n_processos: usize,
    /// Ordenados do mais pesado para o mais leve.
    pub grupos: Vec<GrupoDeProcessos>,
}

impl Amostra {
    pub fn cpu_nao_atribuida(&self) -> f64 {
        self.cpu_total - self.cpu_processos
    }
}

pub struct Coletor {
    consulta: Consulta,
    cpu_por_logico: Contador,
    cpu_desempenho: Contador,
    proc_cpu: Contador,
    proc_privada: Contador,
    proc_ws: Contador,
    proc_es_leitura: Contador,
    proc_es_escrita: Contador,
    mem_standby: [Contador; 3],
    mem_livre: Contador,
    mem_modificada: Contador,
    mem_kernel: [Contador; 5],
    gpu_motor: Contador,
    gpu_adaptador: Contador,
    gpu_processo: Contador,
    disco_ocioso: Contador,
    disco_leitura: Contador,
    disco_escrita: Contador,
    rede_recebido: Contador,
    rede_enviado: Contador,
    total_ram: f64,
    cpu_proprio_antes: u64,
    relogio: Instant,
    /// (caminho do executável, linha de comando) por pid, consultados uma vez por processo.
    identidades: HashMap<u32, (String, String)>,
    /// Gerenciador de serviços do Windows; zero se não abriu.
    gerenciador_de_servicos: Handle,
    /// (tipo de início, conta, linha do binário) por nome de serviço, consultados uma vez.
    configuracoes_de_servico: HashMap<String, (String, String, String)>,
    /// Núcleos físicos com seus lógicos; fixo durante a execução.
    pub nucleos: Vec<Vec<usize>>,
}

fn maior(mapa: &HashMap<String, f64>) -> Option<(String, f64)> {
    mapa.iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(chave, valor)| (chave.clone(), *valor))
}

/// "nome:1234" -> ("nome", 1234). Ignora o total e o processo ocioso.
fn separar_instancia(instancia: &str) -> Option<(&str, u32)> {
    if instancia == "_Total" {
        return None;
    }
    let (nome, pid) = instancia.rsplit_once(':')?;
    if nome == "Idle" {
        return None;
    }
    Some((nome, pid.parse().ok()?))
}

/// "pid_1234_luid_..." -> (1234, "luid_...").
fn separar_gpu(instancia: &str) -> Option<(u32, &str)> {
    let resto = instancia.strip_prefix("pid_")?;
    let (pid, depois) = resto.split_once('_')?;
    Some((pid.parse().ok()?, depois))
}

impl Coletor {
    pub fn novo() -> Coletor {
        let consulta = Consulta::nova();
        let coletor = Coletor {
            cpu_por_logico: consulta.contador(r"\Processor Information(*)\% Processor Time"),
            cpu_desempenho: consulta.contador(r"\Processor Information(_Total)\% Processor Performance"),
            proc_cpu: consulta.contador(r"\Process V2(*)\% Processor Time"),
            proc_privada: consulta.contador(r"\Process V2(*)\Working Set - Private"),
            proc_ws: consulta.contador(r"\Process V2(*)\Working Set"),
            proc_es_leitura: consulta.contador(r"\Process V2(*)\IO Read Bytes/sec"),
            proc_es_escrita: consulta.contador(r"\Process V2(*)\IO Write Bytes/sec"),
            mem_standby: [
                consulta.contador(r"\Memory\Standby Cache Core Bytes"),
                consulta.contador(r"\Memory\Standby Cache Normal Priority Bytes"),
                consulta.contador(r"\Memory\Standby Cache Reserve Bytes"),
            ],
            mem_livre: consulta.contador(r"\Memory\Free & Zero Page List Bytes"),
            mem_modificada: consulta.contador(r"\Memory\Modified Page List Bytes"),
            mem_kernel: [
                consulta.contador(r"\Memory\Pool Nonpaged Bytes"),
                consulta.contador(r"\Memory\Pool Paged Resident Bytes"),
                consulta.contador(r"\Memory\System Code Resident Bytes"),
                consulta.contador(r"\Memory\System Driver Resident Bytes"),
                consulta.contador(r"\Memory\System Cache Resident Bytes"),
            ],
            gpu_motor: consulta.contador(r"\GPU Engine(*)\Utilization Percentage"),
            gpu_adaptador: consulta.contador(r"\GPU Adapter Memory(*)\Dedicated Usage"),
            gpu_processo: consulta.contador(r"\GPU Process Memory(*)\Dedicated Usage"),
            disco_ocioso: consulta.contador(r"\PhysicalDisk(*)\% Idle Time"),
            disco_leitura: consulta.contador(r"\PhysicalDisk(*)\Disk Read Bytes/sec"),
            disco_escrita: consulta.contador(r"\PhysicalDisk(*)\Disk Write Bytes/sec"),
            rede_recebido: consulta.contador(r"\Network Interface(*)\Bytes Received/sec"),
            rede_enviado: consulta.contador(r"\Network Interface(*)\Bytes Sent/sec"),
            total_ram: memoria_fisica_total() as f64,
            cpu_proprio_antes: tempo_de_cpu_proprio(),
            relogio: Instant::now(),
            identidades: HashMap::new(),
            gerenciador_de_servicos: unsafe {
                OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_ENUMERATE_SERVICE)
            },
            configuracoes_de_servico: HashMap::new(),
            nucleos: nucleos_fisicos(),
            consulta,
        };
        // Contadores de taxa precisam de duas coletas; esta é a primeira.
        coletor.consulta.coletar();
        coletor
    }

    /// Serviços em execução, por processo, com a linha do binário de cada um.
    fn servicos_por_processo(&mut self) -> HashMap<u32, Vec<(Servico, String)>> {
        let mut por_pid: HashMap<u32, Vec<(Servico, String)>> = HashMap::new();
        if self.gerenciador_de_servicos == 0 {
            return por_pid;
        }
        let mut buffer: Vec<u64> = vec![0; 32 * 1024]; // 256 KB, o máximo por chamada
        let mut retomada: u32 = 0;
        loop {
            let mut necessario: u32 = 0;
            let mut devolvidos: u32 = 0;
            let ok = unsafe {
                EnumServicesStatusExW(
                    self.gerenciador_de_servicos,
                    0, // SC_ENUM_PROCESS_INFO
                    SERVICE_WIN32,
                    SERVICE_ACTIVE,
                    buffer.as_mut_ptr() as *mut u8,
                    (buffer.len() * 8) as u32,
                    &mut necessario,
                    &mut devolvidos,
                    &mut retomada,
                    ptr::null(),
                )
            };
            let base = buffer.as_ptr() as *const u8;
            for i in 0..devolvidos as usize {
                // Registro: nome (ponteiro), nome de exibição (ponteiro) e o estado, com o pid no byte 44.
                let registro = unsafe { base.add(i * TAMANHO_DO_REGISTRO_DE_SERVICO) };
                let nome = unsafe { texto_largo(*(registro as *const *const u16)) };
                let exibicao = unsafe { texto_largo(*(registro.add(8) as *const *const u16)) };
                let pid = unsafe { *(registro.add(44) as *const u32) };
                if pid == 0 || nome.is_empty() {
                    continue;
                }
                let gerenciador = self.gerenciador_de_servicos;
                let (inicio, conta, binario) = self
                    .configuracoes_de_servico
                    .entry(nome.clone())
                    .or_insert_with(|| configuracao_do_servico(gerenciador, &nome))
                    .clone();
                por_pid.entry(pid).or_default().push((Servico { nome, exibicao, inicio, conta }, binario));
            }
            // Zero com "retomada" diferente de zero significa que há mais serviços a ler.
            if ok != 0 || retomada == 0 || devolvidos == 0 {
                break;
            }
        }
        por_pid
    }

    /// Lê tudo. O intervalo entre chamadas é responsabilidade de quem chama.
    pub fn amostrar(&mut self) -> Amostra {
        let inicio = Instant::now();
        self.consulta.coletar();

        // ---- CPU ----
        let mut por_logico: Vec<f64> = Vec::new();
        let mut cpu_total = 0.0;
        for (nome, valor) in self.cpu_por_logico.instancias() {
            if nome == "_Total" {
                cpu_total = valor;
            } else if !nome.contains("_Total") {
                let indice: usize = nome.rsplit(',').next().and_then(|s| s.parse().ok()).unwrap_or(0);
                if por_logico.len() <= indice {
                    por_logico.resize(indice + 1, 0.0);
                }
                por_logico[indice] = valor;
            }
        }
        let n_logicos = por_logico.len().max(1) as f64;

        // ---- Processos, um a um ----
        let mut membros: HashMap<u32, (String, Membro)> = HashMap::new();
        for (instancia, valor) in self.proc_cpu.instancias() {
            if let Some((nome, pid)) = separar_instancia(&instancia) {
                membros.insert(pid, (nome.to_string(), Membro { pid, cpu: valor / n_logicos, ..Membro::default() }));
            }
        }
        let preencher = |contador: &Contador, membros: &mut HashMap<u32, (String, Membro)>, campo: fn(&mut Membro) -> &mut f64| {
            for (instancia, valor) in contador.instancias() {
                if let Some((_, pid)) = separar_instancia(&instancia) {
                    if let Some((_, membro)) = membros.get_mut(&pid) {
                        *campo(membro) = valor;
                    }
                }
            }
        };
        preencher(&self.proc_privada, &mut membros, |m| &mut m.privada);
        preencher(&self.proc_ws, &mut membros, |m| &mut m.working_set);
        preencher(&self.proc_es_leitura, &mut membros, |m| &mut m.es_leitura);
        preencher(&self.proc_es_escrita, &mut membros, |m| &mut m.es_escrita);

        // Caminho e linha de comando: consulta só os processos novos e esquece os
        // que saíram. Os títulos das janelas mudam, então são lidos a cada amostra.
        self.identidades.retain(|pid, _| membros.contains_key(pid));
        let mut titulos = titulos_das_janelas();
        for (pid, (_, membro)) in membros.iter_mut() {
            let (caminho, comando) =
                self.identidades.entry(*pid).or_insert_with(|| identidade_do_processo(*pid)).clone();
            membro.caminho = caminho;
            membro.comando = comando;
            membro.titulos = titulos.remove(pid).unwrap_or_default();
        }

        // Serviços: quando o Windows não deixa consultar o processo, a
        // configuração do serviço ainda diz qual executável ele roda.
        let mut servicos = self.servicos_por_processo();
        for (pid, (_, membro)) in membros.iter_mut() {
            let Some(lista) = servicos.remove(pid) else { continue };
            if let Some((_, binario)) = lista.first() {
                if membro.caminho.is_empty() {
                    membro.caminho = executavel_do_binario(binario);
                }
                if membro.comando.is_empty() {
                    membro.comando = binario.clone();
                }
            }
            membro.servicos = lista.into_iter().map(|(servico, _)| servico).collect();
        }

        // ---- Memória ----
        let soma = |contadores: &[Contador]| -> f64 { contadores.iter().filter_map(|c| c.valor()).sum() };
        let cache = soma(&self.mem_standby);
        let livre = self.mem_livre.valor().unwrap_or(0.0);
        let modificada = self.mem_modificada.valor().unwrap_or(0.0);
        let windows = soma(&self.mem_kernel);
        let programas: f64 = membros.values().map(|(_, m)| m.privada).sum();
        let em_uso = self.total_ram - cache - livre - modificada;
        let compartilhado_e_outros = em_uso - programas - windows;

        // ---- GPU ----
        let mut por_motor: HashMap<String, f64> = HashMap::new();
        let mut gpu_por_grupo: HashMap<String, HashMap<String, f64>> = HashMap::new();
        for (instancia, valor) in self.gpu_motor.instancias() {
            let Some((pid, motor)) = separar_gpu(&instancia) else { continue };
            *por_motor.entry(motor.to_string()).or_default() += valor;
            let tipo = motor.rsplit_once("engtype_").map(|(_, t)| t).unwrap_or("?");
            if let Some((nome, _)) = membros.get(&pid) {
                *gpu_por_grupo.entry(nome.clone()).or_default().entry(tipo.to_string()).or_default() += valor;
            }
        }
        let gpu = maior(&por_motor).map(|(motor, uso)| {
            let tipo = motor.rsplit_once("engtype_").map(|(_, t)| t.to_string()).unwrap_or_else(|| "?".into());
            (tipo, uso)
        });
        let adaptador = self
            .gpu_adaptador
            .instancias()
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut vram_processos = 0.0;
        if let Some((nome_adaptador, _)) = &adaptador {
            for (instancia, valor) in self.gpu_processo.instancias() {
                let Some((pid, resto)) = separar_gpu(&instancia) else { continue };
                if resto != nome_adaptador {
                    continue;
                }
                vram_processos += valor;
                if let Some((_, membro)) = membros.get_mut(&pid) {
                    membro.vram += valor;
                }
            }
        }

        // ---- Disco e rede ----
        let leitura: HashMap<String, f64> = self.disco_leitura.instancias().into_iter().collect();
        let escrita: HashMap<String, f64> = self.disco_escrita.instancias().into_iter().collect();
        let mut discos: Vec<Disco> = self
            .disco_ocioso
            .instancias()
            .into_iter()
            .filter(|(nome, _)| nome != "_Total")
            .map(|(nome, ocioso)| Disco {
                atividade: (100.0 - ocioso).clamp(0.0, 100.0),
                leitura: leitura.get(&nome).copied().unwrap_or(0.0),
                escrita: escrita.get(&nome).copied().unwrap_or(0.0),
                nome,
            })
            .collect();
        discos.sort_by(|a, b| a.nome.cmp(&b.nome));

        let enviado: HashMap<String, f64> = self.rede_enviado.instancias().into_iter().collect();
        let mut redes: Vec<Rede> = self
            .rede_recebido
            .instancias()
            .into_iter()
            .map(|(nome, recebido)| Rede { enviado: enviado.get(&nome).copied().unwrap_or(0.0), recebido, nome })
            .collect();
        redes.sort_by(|a, b| a.nome.cmp(&b.nome));

        // ---- Grupos, do mais pesado para o mais leve ----
        let n_processos = membros.len();
        let mut por_nome: HashMap<String, Vec<Membro>> = HashMap::new();
        for (_, (nome, membro)) in membros {
            por_nome.entry(nome).or_default().push(membro);
        }
        let mut grupos: Vec<GrupoDeProcessos> = por_nome
            .into_iter()
            .map(|(nome, mut membros)| {
                membros.sort_by(|a, b| b.privada.partial_cmp(&a.privada).unwrap_or(std::cmp::Ordering::Equal));
                let soma = |campo: fn(&Membro) -> f64| -> f64 { membros.iter().map(campo).sum() };
                GrupoDeProcessos {
                    cpu: soma(|m| m.cpu),
                    privada: soma(|m| m.privada),
                    working_set: soma(|m| m.working_set),
                    vram: soma(|m| m.vram),
                    es_leitura: soma(|m| m.es_leitura),
                    es_escrita: soma(|m| m.es_escrita),
                    gpu: gpu_por_grupo.get(&nome).and_then(maior).filter(|(_, uso)| *uso >= 0.05),
                    nome,
                    membros,
                }
            })
            .collect();
        // Peso: a maior entre a fatia de CPU e a fatia de memória do grupo.
        let total_ram = self.total_ram;
        let cpu_processos: f64 = grupos.iter().map(|g| g.cpu).sum();
        let peso = |g: &GrupoDeProcessos| (g.cpu / 100.0).max(g.privada / total_ram);
        grupos.sort_by(|a, b| peso(b).partial_cmp(&peso(a)).unwrap_or(std::cmp::Ordering::Equal));

        let ms_coleta = inicio.elapsed().as_secs_f64() * 1000.0;

        // ---- Custo do próprio coletor ----
        let cpu_proprio_agora = tempo_de_cpu_proprio();
        let decorrido = self.relogio.elapsed().as_secs_f64().max(0.001);
        self.relogio = Instant::now();
        let custo_cpu =
            (cpu_proprio_agora - self.cpu_proprio_antes) as f64 / 1e7 / decorrido / n_logicos * 100.0;
        self.cpu_proprio_antes = cpu_proprio_agora;

        Amostra {
            decorrido,
            custo_cpu,
            custo_mem_mb: working_set_proprio() as f64 / (1024.0 * 1024.0),
            ms_coleta,
            cpu_total,
            turbo: self.cpu_desempenho.valor(),
            por_logico,
            cpu_processos,
            total_ram,
            em_uso,
            programas,
            windows,
            compartilhado_e_outros,
            cache,
            modificada,
            livre,
            gpu,
            vram_dedicada: adaptador.map(|(_, bytes)| bytes),
            vram_processos,
            discos,
            redes,
            n_processos,
            grupos,
        }
    }
}
