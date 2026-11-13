//! O que a janela mostra e em que ordem. Tem um padrão e é salvo em
//! `%APPDATA%\Prumo\config.txt` quando o usuário muda algo.

use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Secao {
    Resumo,
    Memoria,
    Nucleos,
    LinhaDoTempo,
    DiscoERede,
}

impl Secao {
    pub const TODAS: [Secao; 5] =
        [Secao::Resumo, Secao::Memoria, Secao::Nucleos, Secao::LinhaDoTempo, Secao::DiscoERede];

    pub fn nome(self) -> &'static str {
        match self {
            Secao::Resumo => "Resumo",
            Secao::Memoria => "Memória",
            Secao::Nucleos => "Núcleos",
            Secao::LinhaDoTempo => "Linha do tempo",
            Secao::DiscoERede => "Disco e rede",
        }
    }

    fn chave(self) -> &'static str {
        match self {
            Secao::Resumo => "resumo",
            Secao::Memoria => "memoria",
            Secao::Nucleos => "nucleos",
            Secao::LinhaDoTempo => "linha",
            Secao::DiscoERede => "disco_rede",
        }
    }

    fn da_chave(chave: &str) -> Option<Secao> {
        Secao::TODAS.iter().copied().find(|s| s.chave() == chave)
    }
}

/// Colunas numéricas da lista de processos, na ordem em que aparecem.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Coluna {
    Cpu,
    Privada,
    Compartilhado,
    Es,
    Gpu,
    Vram,
}

impl Coluna {
    pub const TODAS: [Coluna; 6] =
        [Coluna::Cpu, Coluna::Privada, Coluna::Compartilhado, Coluna::Es, Coluna::Gpu, Coluna::Vram];

    pub fn nome(self) -> &'static str {
        match self {
            Coluna::Cpu => "CPU",
            Coluna::Privada => "Privada",
            Coluna::Compartilhado => "Com compart.",
            Coluna::Es => "E/S",
            Coluna::Gpu => "GPU",
            Coluna::Vram => "VRAM",
        }
    }

    pub fn largura(self) -> f32 {
        match self {
            Coluna::Cpu => 70.0,
            Coluna::Privada => 90.0,
            Coluna::Compartilhado => 115.0,
            Coluna::Es => 100.0,
            Coluna::Gpu => 110.0,
            Coluna::Vram => 90.0,
        }
    }

    pub fn da_chave(chave: &str) -> Option<Coluna> {
        Coluna::TODAS.iter().copied().find(|c| c.chave() == chave)
    }

    pub fn chave(self) -> &'static str {
        match self {
            Coluna::Cpu => "cpu",
            Coluna::Privada => "privada",
            Coluna::Compartilhado => "compartilhado",
            Coluna::Es => "es",
            Coluna::Gpu => "gpu",
            Coluna::Vram => "vram",
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Config {
    /// Seções acima da lista de processos, na ordem de exibição, com a visibilidade.
    pub secoes: Vec<(Secao, bool)>,
    /// A lista de processos fica sempre por último e ocupa o que sobrar.
    pub processos: bool,
    /// Visibilidade de cada coluna, na ordem de `Coluna::TODAS`.
    pub colunas: [bool; 6],
    /// Ordenação da lista: "peso", "nome" ou a chave de uma coluna.
    pub ordem: String,
    pub decrescente: bool,
    /// Libera prioridade e encerramento em processos do Windows. Os essenciais
    /// continuam bloqueados mesmo assim.
    pub liberar_windows: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            secoes: Secao::TODAS.iter().map(|s| (*s, true)).collect(),
            processos: true,
            colunas: [true; 6],
            ordem: "peso".into(),
            decrescente: true,
            liberar_windows: false,
        }
    }
}

fn arquivo() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("Prumo").join("config.txt"))
}

impl Config {
    pub fn coluna_visivel(&self, coluna: Coluna) -> bool {
        Coluna::TODAS.iter().position(|c| *c == coluna).map(|i| self.colunas[i]).unwrap_or(false)
    }

    pub fn colunas_visiveis(&self) -> Vec<Coluna> {
        Coluna::TODAS.iter().copied().filter(|c| self.coluna_visivel(*c)).collect()
    }

    /// Lê o arquivo; o que faltar ou estiver errado fica no padrão.
    pub fn carregar() -> Config {
        let mut config = Config::default();
        let Some(texto) = arquivo().and_then(|caminho| fs::read_to_string(caminho).ok()) else {
            return config;
        };
        for linha in texto.lines() {
            let Some((chave, valor)) = linha.split_once('=') else { continue };
            let pares = || valor.split(',').filter_map(|par| par.split_once(':')).map(|(nome, v)| (nome.trim(), v.trim() == "1"));
            match chave.trim() {
                "secoes" => {
                    let mut lidas: Vec<(Secao, bool)> = Vec::new();
                    for (nome, visivel) in pares() {
                        if let Some(secao) = Secao::da_chave(nome) {
                            if !lidas.iter().any(|(s, _)| *s == secao) {
                                lidas.push((secao, visivel));
                            }
                        }
                    }
                    // Seção que o arquivo não conhece entra no fim, visível.
                    for secao in Secao::TODAS {
                        if !lidas.iter().any(|(s, _)| *s == secao) {
                            lidas.push((secao, true));
                        }
                    }
                    config.secoes = lidas;
                }
                "colunas" => {
                    for (nome, visivel) in pares() {
                        if let Some(i) = Coluna::TODAS.iter().position(|c| c.chave() == nome) {
                            config.colunas[i] = visivel;
                        }
                    }
                }
                "processos" => config.processos = valor.trim() == "1",
                "ordem" => config.ordem = valor.trim().to_string(),
                "decrescente" => config.decrescente = valor.trim() == "1",
                "liberar_windows" => config.liberar_windows = valor.trim() == "1",
                _ => {}
            }
        }
        config
    }

    pub fn salvar(&self) {
        let Some(caminho) = arquivo() else { return };
        if let Some(pasta) = caminho.parent() {
            let _ = fs::create_dir_all(pasta);
        }
        let marca = |v: bool| if v { "1" } else { "0" };
        let secoes: Vec<String> = self.secoes.iter().map(|(s, v)| format!("{}:{}", s.chave(), marca(*v))).collect();
        let colunas: Vec<String> =
            Coluna::TODAS.iter().zip(self.colunas).map(|(c, v)| format!("{}:{}", c.chave(), marca(v))).collect();
        let texto = format!(
            "secoes={}\ncolunas={}\nprocessos={}\nordem={}\ndecrescente={}\nliberar_windows={}\n",
            secoes.join(","),
            colunas.join(","),
            marca(self.processos),
            self.ordem,
            marca(self.decrescente),
            marca(self.liberar_windows)
        );
        let _ = fs::write(caminho, texto);
    }
}
