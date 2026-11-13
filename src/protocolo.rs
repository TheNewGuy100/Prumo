//! Conversa entre o coletor e a janela, por um pipe nomeado local.
//!
//! Cada mensagem é um quadro: tipo (1 byte), tamanho (u32) e corpo. O coletor
//! manda uma apresentação ao conectar e depois uma amostra por intervalo. A
//! janela só lê.

use std::io::{self, Read};

use crate::coletor::{Amostra, Disco, GrupoDeProcessos, Membro, Rede, Servico};

pub const NOME_DO_PIPE: &str = r"\\.\pipe\prumo-coletor";
pub const NOME_DO_EVENTO_DE_PARADA: &str = r"Local\PrumoColetorParar";
pub const VERSAO: u32 = 4;

pub const QUADRO_APRESENTACAO: u8 = 1;
pub const QUADRO_AMOSTRA: u8 = 2;

const TAMANHO_MAXIMO: usize = 16 * 1024 * 1024;

/// Primeira mensagem: o que a janela precisa para já abrir com histórico.
pub struct Apresentacao {
    pub nucleos: Vec<Vec<usize>>,
    pub historico_cpu: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Escrita e leitura de campos
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Escritor {
    dados: Vec<u8>,
}

impl Escritor {
    fn u8(&mut self, v: u8) {
        self.dados.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.dados.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.dados.extend_from_slice(&v.to_le_bytes());
    }
    fn texto(&mut self, v: &str) {
        self.u32(v.len() as u32);
        self.dados.extend_from_slice(v.as_bytes());
    }
    fn opcional(&mut self, v: Option<f64>) {
        match v {
            Some(valor) => {
                self.u8(1);
                self.f64(valor);
            }
            None => self.u8(0),
        }
    }
    fn tipo_e_uso(&mut self, v: &Option<(String, f64)>) {
        match v {
            Some((tipo, uso)) => {
                self.u8(1);
                self.texto(tipo);
                self.f64(*uso);
            }
            None => self.u8(0),
        }
    }
}

struct Leitor<'a> {
    dados: &'a [u8],
    pos: usize,
}

impl<'a> Leitor<'a> {
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let fim = self.pos.checked_add(n)?;
        let fatia = self.dados.get(self.pos..fim)?;
        self.pos = fim;
        Some(fatia)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.bytes(1)?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.bytes(8)?.try_into().ok()?))
    }
    fn texto(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        String::from_utf8(self.bytes(n)?.to_vec()).ok()
    }
    fn opcional(&mut self) -> Option<Option<f64>> {
        Some(if self.u8()? == 1 { Some(self.f64()?) } else { None })
    }
    fn tipo_e_uso(&mut self) -> Option<Option<(String, f64)>> {
        Some(if self.u8()? == 1 { Some((self.texto()?, self.f64()?)) } else { None })
    }
    /// Quantidade de itens de uma lista, limitada pelo que cabe no corpo.
    fn quantidade(&mut self) -> Option<usize> {
        let n = self.u32()? as usize;
        if n > self.dados.len() {
            return None;
        }
        Some(n)
    }
}

// ---------------------------------------------------------------------------
// Mensagens
// ---------------------------------------------------------------------------

impl Apresentacao {
    pub fn codificar(&self) -> Vec<u8> {
        let mut e = Escritor::default();
        e.u32(VERSAO);
        e.u32(self.nucleos.len() as u32);
        for logicos in &self.nucleos {
            e.u32(logicos.len() as u32);
            for l in logicos {
                e.u32(*l as u32);
            }
        }
        e.u32(self.historico_cpu.len() as u32);
        for v in &self.historico_cpu {
            e.f64(*v as f64);
        }
        e.dados
    }

    pub fn decodificar(dados: &[u8]) -> Option<Apresentacao> {
        let mut l = Leitor { dados, pos: 0 };
        if l.u32()? != VERSAO {
            return None;
        }
        let mut nucleos = Vec::new();
        for _ in 0..l.quantidade()? {
            let mut logicos = Vec::new();
            for _ in 0..l.quantidade()? {
                logicos.push(l.u32()? as usize);
            }
            nucleos.push(logicos);
        }
        let mut historico_cpu = Vec::new();
        for _ in 0..l.quantidade()? {
            historico_cpu.push(l.f64()? as f32);
        }
        Some(Apresentacao { nucleos, historico_cpu })
    }
}

impl Amostra {
    pub fn codificar(&self) -> Vec<u8> {
        let mut e = Escritor::default();
        for v in [self.decorrido, self.custo_cpu, self.custo_mem_mb, self.ms_coleta, self.cpu_total] {
            e.f64(v);
        }
        e.opcional(self.turbo);
        e.u32(self.por_logico.len() as u32);
        for v in &self.por_logico {
            e.f64(*v);
        }
        for v in [
            self.cpu_processos,
            self.total_ram,
            self.em_uso,
            self.programas,
            self.windows,
            self.compartilhado_e_outros,
            self.cache,
            self.modificada,
            self.livre,
        ] {
            e.f64(v);
        }
        e.tipo_e_uso(&self.gpu);
        e.opcional(self.vram_dedicada);
        e.f64(self.vram_processos);
        e.u32(self.discos.len() as u32);
        for d in &self.discos {
            e.texto(&d.nome);
            e.f64(d.atividade);
            e.f64(d.leitura);
            e.f64(d.escrita);
        }
        e.u32(self.redes.len() as u32);
        for r in &self.redes {
            e.texto(&r.nome);
            e.f64(r.recebido);
            e.f64(r.enviado);
        }
        e.u32(self.n_processos as u32);
        e.u32(self.grupos.len() as u32);
        for g in &self.grupos {
            e.texto(&g.nome);
            e.u32(g.membros.len() as u32);
            for m in &g.membros {
                e.u32(m.pid);
                for v in [m.cpu, m.privada, m.working_set, m.vram, m.es_leitura, m.es_escrita] {
                    e.f64(v);
                }
                e.texto(&m.caminho);
                e.texto(&m.comando);
                e.u32(m.titulos.len() as u32);
                for titulo in &m.titulos {
                    e.texto(titulo);
                }
                e.u32(m.servicos.len() as u32);
                for s in &m.servicos {
                    e.texto(&s.nome);
                    e.texto(&s.exibicao);
                    e.texto(&s.inicio);
                    e.texto(&s.conta);
                }
            }
            for v in [g.cpu, g.privada, g.working_set] {
                e.f64(v);
            }
            e.tipo_e_uso(&g.gpu);
            for v in [g.vram, g.es_leitura, g.es_escrita] {
                e.f64(v);
            }
        }
        e.dados
    }

    pub fn decodificar(dados: &[u8]) -> Option<Amostra> {
        let mut l = Leitor { dados, pos: 0 };
        let decorrido = l.f64()?;
        let custo_cpu = l.f64()?;
        let custo_mem_mb = l.f64()?;
        let ms_coleta = l.f64()?;
        let cpu_total = l.f64()?;
        let turbo = l.opcional()?;
        let mut por_logico = Vec::new();
        for _ in 0..l.quantidade()? {
            por_logico.push(l.f64()?);
        }
        let cpu_processos = l.f64()?;
        let total_ram = l.f64()?;
        let em_uso = l.f64()?;
        let programas = l.f64()?;
        let windows = l.f64()?;
        let compartilhado_e_outros = l.f64()?;
        let cache = l.f64()?;
        let modificada = l.f64()?;
        let livre = l.f64()?;
        let gpu = l.tipo_e_uso()?;
        let vram_dedicada = l.opcional()?;
        let vram_processos = l.f64()?;
        let mut discos = Vec::new();
        for _ in 0..l.quantidade()? {
            discos.push(Disco { nome: l.texto()?, atividade: l.f64()?, leitura: l.f64()?, escrita: l.f64()? });
        }
        let mut redes = Vec::new();
        for _ in 0..l.quantidade()? {
            redes.push(Rede { nome: l.texto()?, recebido: l.f64()?, enviado: l.f64()? });
        }
        let n_processos = l.u32()? as usize;
        let mut grupos = Vec::new();
        for _ in 0..l.quantidade()? {
            let nome = l.texto()?;
            let mut membros = Vec::new();
            for _ in 0..l.quantidade()? {
                let mut membro = Membro {
                    pid: l.u32()?,
                    cpu: l.f64()?,
                    privada: l.f64()?,
                    working_set: l.f64()?,
                    vram: l.f64()?,
                    es_leitura: l.f64()?,
                    es_escrita: l.f64()?,
                    caminho: l.texto()?,
                    comando: l.texto()?,
                    titulos: Vec::new(),
                    servicos: Vec::new(),
                };
                for _ in 0..l.quantidade()? {
                    membro.titulos.push(l.texto()?);
                }
                for _ in 0..l.quantidade()? {
                    membro.servicos.push(Servico {
                        nome: l.texto()?,
                        exibicao: l.texto()?,
                        inicio: l.texto()?,
                        conta: l.texto()?,
                    });
                }
                membros.push(membro);
            }
            grupos.push(GrupoDeProcessos {
                nome,
                membros,
                cpu: l.f64()?,
                privada: l.f64()?,
                working_set: l.f64()?,
                gpu: l.tipo_e_uso()?,
                vram: l.f64()?,
                es_leitura: l.f64()?,
                es_escrita: l.f64()?,
            });
        }
        Some(Amostra {
            decorrido,
            custo_cpu,
            custo_mem_mb,
            ms_coleta,
            cpu_total,
            turbo,
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
            vram_dedicada,
            vram_processos,
            discos,
            redes,
            n_processos,
            grupos,
        })
    }
}

// ---------------------------------------------------------------------------
// Quadros
// ---------------------------------------------------------------------------

/// Monta um quadro completo, pronto para escrever no pipe.
pub fn montar_quadro(tipo: u8, corpo: &[u8]) -> Vec<u8> {
    let mut quadro = Vec::with_capacity(corpo.len() + 5);
    quadro.push(tipo);
    quadro.extend_from_slice(&(corpo.len() as u32).to_le_bytes());
    quadro.extend_from_slice(corpo);
    quadro
}

pub fn ler_quadro(leitor: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut cabecalho = [0u8; 5];
    leitor.read_exact(&mut cabecalho)?;
    let tamanho = u32::from_le_bytes([cabecalho[1], cabecalho[2], cabecalho[3], cabecalho[4]]) as usize;
    if tamanho > TAMANHO_MAXIMO {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "quadro grande demais"));
    }
    let mut corpo = vec![0u8; tamanho];
    leitor.read_exact(&mut corpo)?;
    Ok((cabecalho[0], corpo))
}
