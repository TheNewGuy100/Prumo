//! Controle de processos: classe de prioridade e encerramento.
//!
//! Regras (PROJETO.md, riscos 5 e 7): processos essenciais nunca são tocados,
//! processos do Windows só com a opção liberada na configuração, "Tempo real"
//! não é oferecido e uma recusa do Windows é aceita sem insistir.

type Handle = isize;

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(acesso: u32, herdar: i32, pid: u32) -> Handle;
    fn SetPriorityClass(processo: Handle, classe: u32) -> i32;
    fn GetPriorityClass(processo: Handle) -> u32;
    fn TerminateProcess(processo: Handle, codigo: u32) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}

const PROCESS_SET_INFORMATION: u32 = 0x0200;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

#[derive(Clone, Copy, PartialEq)]
pub enum Prioridade {
    Baixa,
    AbaixoDoNormal,
    Normal,
    AcimaDoNormal,
    Alta,
}

pub const PRIORIDADES: [Prioridade; 5] = [
    Prioridade::Alta,
    Prioridade::AcimaDoNormal,
    Prioridade::Normal,
    Prioridade::AbaixoDoNormal,
    Prioridade::Baixa,
];

impl Prioridade {
    fn classe(self) -> u32 {
        match self {
            Prioridade::Baixa => 0x0000_0040,
            Prioridade::AbaixoDoNormal => 0x0000_4000,
            Prioridade::Normal => 0x0000_0020,
            Prioridade::AcimaDoNormal => 0x0000_8000,
            Prioridade::Alta => 0x0000_0080,
        }
    }

    fn da_classe(classe: u32) -> Option<Prioridade> {
        PRIORIDADES.iter().copied().find(|p| p.classe() == classe)
    }

    pub fn nome(self) -> &'static str {
        match self {
            Prioridade::Baixa => "Baixa",
            Prioridade::AbaixoDoNormal => "Abaixo do normal",
            Prioridade::Normal => "Normal",
            Prioridade::AcimaDoNormal => "Acima do normal",
            Prioridade::Alta => "Alta",
        }
    }

    pub fn do_argumento(texto: &str) -> Option<Prioridade> {
        match texto {
            "baixa" => Some(Prioridade::Baixa),
            "abaixo" => Some(Prioridade::AbaixoDoNormal),
            "normal" => Some(Prioridade::Normal),
            "acima" => Some(Prioridade::AcimaDoNormal),
            "alta" => Some(Prioridade::Alta),
            _ => None,
        }
    }
}

/// Quanto o Prumo deixa mexer num processo.
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub enum Protecao {
    /// Programa comum: prioridade e encerramento liberados.
    Livre,
    /// Processo do Windows: bloqueado, a menos que o usuário libere na configuração.
    Windows,
    /// Essencial: encerrar derruba ou trava o sistema. Nunca é liberado.
    Critico,
}

/// Processos sem os quais o Windows cai ou encerra a sessão.
const CRITICOS: [&str; 11] = [
    "System",
    "Registry",
    "Secure System",
    "Memory Compression",
    "smss",
    "csrss",
    "wininit",
    "winlogon",
    "services",
    "lsass",
    "lsaiso",
];

/// Classifica um processo pelo nome e pelo caminho do executável.
///
/// É uma trava contra engano, não uma barreira de segurança: quem decide de
/// fato se um processo pode ser encerrado é o Windows.
pub fn protecao(nome: &str, caminho: &str) -> Protecao {
    if CRITICOS.iter().any(|c| c.eq_ignore_ascii_case(nome)) {
        return Protecao::Critico;
    }
    // Sem caminho, o Windows não deixou consultar: é processo protegido do sistema.
    if caminho.is_empty() {
        return Protecao::Windows;
    }
    let raiz = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()).to_lowercase();
    let caminho = caminho.to_lowercase();
    if caminho.starts_with(&format!("{raiz}\\")) {
        return Protecao::Windows;
    }
    Protecao::Livre
}

/// Se a ação é permitida, dado o nível do processo e a opção do usuário.
pub fn permitido(protecao: Protecao, liberar_windows: bool) -> bool {
    match protecao {
        Protecao::Livre => true,
        Protecao::Windows => liberar_windows,
        Protecao::Critico => false,
    }
}

const PROCESS_TERMINATE: u32 = 0x0001;

/// Se o Windows deixa este usuário alterar e encerrar o processo. Só testa a
/// permissão: abre e fecha, sem mudar nada.
pub fn acessivel(pid: u32) -> bool {
    let processo = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SET_INFORMATION, 0, pid) };
    if processo == 0 {
        return false;
    }
    unsafe { CloseHandle(processo) };
    true
}

/// Encerra cada processo, sem dar chance de salvar. Devolve (encerrados, recusados).
/// Não encerra o próprio processo.
pub fn encerrar(pids: &[u32]) -> (usize, usize) {
    let proprio = std::process::id();
    let mut encerrados = 0;
    let mut recusados = 0;
    for pid in pids {
        if *pid == proprio {
            recusados += 1;
            continue;
        }
        let processo = unsafe { OpenProcess(PROCESS_TERMINATE, 0, *pid) };
        if processo == 0 {
            recusados += 1;
            continue;
        }
        if unsafe { TerminateProcess(processo, 1) } != 0 {
            encerrados += 1;
        } else {
            recusados += 1;
        }
        unsafe { CloseHandle(processo) };
    }
    (encerrados, recusados)
}

/// Prioridade atual de um processo, se o Windows deixar consultar.
pub fn prioridade_atual(pid: u32) -> Option<Prioridade> {
    let processo = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if processo == 0 {
        return None;
    }
    let classe = unsafe { GetPriorityClass(processo) };
    unsafe { CloseHandle(processo) };
    Prioridade::da_classe(classe)
}

/// Aplica a prioridade a cada processo. Devolve (aplicados, recusados).
pub fn definir_prioridade(pids: &[u32], prioridade: Prioridade) -> (usize, usize) {
    let mut aplicados = 0;
    let mut recusados = 0;
    for pid in pids {
        let processo = unsafe { OpenProcess(PROCESS_SET_INFORMATION, 0, *pid) };
        if processo == 0 {
            recusados += 1;
            continue;
        }
        if unsafe { SetPriorityClass(processo, prioridade.classe()) } != 0 {
            aplicados += 1;
        } else {
            recusados += 1;
        }
        unsafe { CloseHandle(processo) };
    }
    (aplicados, recusados)
}
