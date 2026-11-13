//! Janela principal, no modo fixo. Segue a seção "Telas" do PROJETO.md.
//!
//! A janela não mede nada: lê as amostras que o coletor publica no pipe e
//! inicia o coletor se ele não estiver rodando. Só redesenha quando chega uma
//! amostra nova ou há interação.

use std::collections::{HashSet, VecDeque};
use std::fs::OpenOptions;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};

use crate::coletor::{Amostra, GrupoDeProcessos, Medidor};
use crate::config::{Coluna, Config, Secao};
use crate::controle::{self, Prioridade, Protecao, PRIORIDADES};
use crate::protocolo::{ler_quadro, Apresentacao, NOME_DO_PIPE, QUADRO_AMOSTRA, QUADRO_APRESENTACAO};
use crate::servico::HISTORICO;
use crate::texto::{gb, num, tamanho, taxa};

const AZUL: Color32 = Color32::from_rgb(0x39, 0x87, 0xe5);
const LARANJA: Color32 = Color32::from_rgb(0xd9, 0x59, 0x26);
const VERDE: Color32 = Color32::from_rgb(0x19, 0x9e, 0x70);
const CINZA: Color32 = Color32::from_rgb(0x88, 0x87, 0x80);
const CINZA_ESCURO: Color32 = Color32::from_rgb(0x5f, 0x5e, 0x5a);
const VERMELHO: Color32 = Color32::from_rgb(0xe6, 0x67, 0x67);

const ALTURA_LINHA: f32 = 22.0;
/// Largura da coluna de disco e rede, à direita da lista de processos.
const LARGURA_LATERAL: f32 = 220.0;
/// Abaixo desta largura, disco e rede saem do lado e vão para cima da lista.
const LARGURA_MINIMA_COM_LATERAL: f32 = 940.0;
/// Espaço à esquerda do nome, onde fica a marca de gaveta.
const RECUO_DO_NOME: f32 = 20.0;
/// Serviços listados na gaveta antes de resumir o resto.
const SERVICOS_NA_GAVETA: usize = 8;
/// Linhas de comando diferentes mostradas na gaveta.
const COMANDOS_NA_GAVETA: usize = 4;
/// Processos listados dentro da gaveta antes de resumir o resto.
const MEMBROS_NA_GAVETA: usize = 12;
/// Linhas que a lista de processos mostra mesmo quando a janela é baixa.
const LINHAS_MINIMAS: f32 = 8.0;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ---------------------------------------------------------------------------
// Textos de ajuda. As definições completas estão em GLOSSARIO.md.
// ---------------------------------------------------------------------------

const DICA_CPU: &str = "Fração do tempo em que os processadores lógicos estavam ocupados, na média do intervalo.\n\nÉ a medida de tempo, não a de \"utility\": a frequência (turbo) aparece à parte, para não inflar o número. Por isso pode diferir de outras ferramentas.";
const DICA_MEMORIA_EM_USO: &str = "Total menos cache liberável, livre e modificada.\n\nInclui a memória dos programas, a do Windows e a compartilhada. A lista de processos sozinha não soma este valor.";
const DICA_GPU: &str = "A placa tem vários motores independentes (3D, vídeo, cópia). Este é o mais ocupado, que é o gargalo real.\n\nO driver da placa pode mostrar outro número, porque mede de outro jeito.";
const DICA_VRAM: &str = "Memória de vídeo dedicada em uso, pelo que o adaptador reporta.\n\nA soma dos processos pode passar deste valor, porque parte é contada em mais de um processo.";
const DICA_MEMORIA: &str = "Toda a RAM dividida em partes que somam o total.\n\nA parte hachurada não é medida: é o que sobra depois de descontar as outras.";
const DICA_PROGRAMAS: &str = "Soma da memória privada de todos os processos: o que só cada um usa.";
const DICA_WINDOWS: &str = "Memória do próprio sistema: pools do kernel, cache do sistema, código e drivers residentes.";
const DICA_COMPARTILHADO: &str = "Calculado por diferença: em uso menos programas menos Windows.\n\nÉ quase todo memória compartilhada entre processos (DLLs, arquivos mapeados), mais estruturas do kernel que o Windows não detalha.";
const DICA_CACHE: &str = "Conteúdo de arquivos mantido na RAM para acelerar. O Windows descarta na hora se um programa precisar, então conta como disponível.";
const DICA_MODIFICADA: &str = "Páginas alteradas que aguardam escrita em disco antes de poderem ser reaproveitadas.";
const DICA_LIVRE: &str = "Memória sem uso nenhum. Pouca memória livre é normal: o Windows prefere usá-la como cache.";
const DICA_NUCLEOS: &str = "Cada par de barras é um núcleo físico com seus dois processadores lógicos.\n\nO segundo lógico de um núcleo rende bem menos que o primeiro. Com um lógico cheio em cada núcleo, o total marca perto de 50%, mas a maior parte da capacidade real já está em uso.";
const DICA_LINHA: &str = "Uso de CPU a cada intervalo, no último minuto. O coletor guarda o histórico mesmo com a janela fechada.";
const DICA_DISCO: &str = "Atividade: fração do tempo em que o disco estava atendendo pedidos. Não é o espaço ocupado.\n\nLeitura e escrita são a taxa de dados no intervalo.";
const DICA_REDE: &str = "Taxa de dados recebidos e enviados por interface.\n\nO Windows não informa rede por processo sem captura elevada; a coluna E/S da lista soma disco, rede e dispositivos.";
const DICA_PROCESSOS: &str = "Processos de mesmo nome aparecem somados. Clique numa linha para abrir ou fechar os detalhes logo abaixo dela; várias podem ficar abertas. Botão direito para o controle.";

fn dica_da_coluna(coluna: Coluna) -> &'static str {
    match coluna {
        Coluna::Cpu => "Tempo de CPU do programa sobre o tempo total da máquina (0 a 100%).\n\nUm programa que ocupa um processador lógico inteiro, de 16, marca 6,25%.",
        Coluna::Privada => "Memória na RAM que só este programa usa. É o que volta a ficar livre se ele fechar.\n\nEsta coluna é somável.",
        Coluna::Compartilhado => "Privada mais as páginas compartilhadas que o programa toca (working set).\n\nNão é somável: a mesma página conta em todos os programas que a usam.",
        Coluna::Es => "Entrada e saída por segundo, leitura mais escrita.\n\nInclui disco, rede e dispositivos; o Windows não separa por tipo neste contador.",
        Coluna::Gpu => "O motor de GPU que o programa mais usa, e quanto.",
        Coluna::Vram => "Memória de vídeo dedicada do programa.\n\nA soma pode passar do total do adaptador.",
    }
}

// ---------------------------------------------------------------------------
// Estado
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Estado {
    atual: Option<Arc<Amostra>>,
    historico_cpu: VecDeque<f32>,
    nucleos: Vec<Vec<usize>>,
    conectado: bool,
    /// (% de CPU, MB) do processo da janela.
    custo_janela: (f64, f64),
}

#[derive(Clone, Copy, PartialEq)]
enum Ordem {
    /// A maior entre a fatia de CPU e a fatia de memória; é a ordem do coletor.
    Peso,
    Nome,
    Por(Coluna),
}

fn ordem_da_config(config: &Config) -> Ordem {
    match config.ordem.as_str() {
        "nome" => Ordem::Nome,
        chave => Coluna::da_chave(chave).map(Ordem::Por).unwrap_or(Ordem::Peso),
    }
}

fn chave_da_ordem(ordem: Ordem) -> &'static str {
    match ordem {
        Ordem::Peso => "peso",
        Ordem::Nome => "nome",
        Ordem::Por(coluna) => coluna.chave(),
    }
}

struct Janela {
    estado: Arc<Mutex<Estado>>,
    config: Config,
    config_aberta: bool,
    filtro: String,
    ordem: Ordem,
    decrescente: bool,
    /// Resultado da última ação de controle.
    aviso: Option<String>,
    /// Programas com a gaveta de detalhes aberta na lista.
    expandidos: HashSet<String>,
    /// Encerramento aguardando confirmação: (o que é, processos).
    confirmar: Option<(String, Vec<u32>)>,
}

/// `abertos` são nomes de programas que já começam com os detalhes abertos;
/// `filtro` é o texto inicial do campo de filtro.
pub fn abrir(intervalo_ms: u64, abertos: Vec<String>, filtro: String) -> Result<(), eframe::Error> {
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Prumo")
            .with_inner_size([1060.0, 820.0])
            .with_min_inner_size([680.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Prumo",
        opcoes,
        Box::new(move |cc| {
            let estado = Arc::new(Mutex::new(Estado::default()));
            let ctx = cc.egui_ctx.clone();
            let compartilhado = estado.clone();
            thread::spawn(move || ler_do_coletor(compartilhado, ctx, intervalo_ms));
            let config = Config::carregar();
            Ok(Box::new(Janela {
                estado,
                ordem: ordem_da_config(&config),
                decrescente: config.decrescente,
                config,
                config_aberta: false,
                filtro,
                aviso: None,
                expandidos: abertos.into_iter().collect(),
                confirmar: None,
            }))
        }),
    )
}

/// Inicia o coletor como processo separado, sem janela de console. Iniciado
/// assim, ele sai sozinho pouco depois que a última janela fechar.
fn iniciar_coletor(intervalo_ms: u64) {
    let Ok(executavel) = std::env::current_exe() else { return };
    let _ = Command::new(executavel)
        .args(["--coletor", "--sair-sem-clientes", "--intervalo-ms", &intervalo_ms.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

fn ler_do_coletor(estado: Arc<Mutex<Estado>>, ctx: egui::Context, intervalo_ms: u64) {
    let mut medidor = Medidor::novo();
    let mut tentou_iniciar = false;
    loop {
        let mut pipe = match OpenOptions::new().read(true).open(NOME_DO_PIPE) {
            Ok(pipe) => pipe,
            Err(_) => {
                if !tentou_iniciar {
                    iniciar_coletor(intervalo_ms);
                    tentou_iniciar = true;
                }
                thread::sleep(Duration::from_millis(300));
                continue;
            }
        };
        tentou_iniciar = false;

        while let Ok((tipo, corpo)) = ler_quadro(&mut pipe) {
            match tipo {
                QUADRO_APRESENTACAO => {
                    if let Some(apresentacao) = Apresentacao::decodificar(&corpo) {
                        let mut e = estado.lock().unwrap();
                        e.nucleos = apresentacao.nucleos;
                        e.historico_cpu = apresentacao.historico_cpu.into();
                        e.conectado = true;
                    }
                }
                QUADRO_AMOSTRA => {
                    if let Some(amostra) = Amostra::decodificar(&corpo) {
                        let custo = medidor.medir();
                        let mut e = estado.lock().unwrap();
                        while e.historico_cpu.len() >= HISTORICO {
                            e.historico_cpu.pop_front();
                        }
                        e.historico_cpu.push_back(amostra.cpu_total as f32);
                        e.atual = Some(Arc::new(amostra));
                        e.custo_janela = custo;
                    }
                }
                _ => {}
            }
            ctx.request_repaint();
        }

        estado.lock().unwrap().conectado = false;
        ctx.request_repaint();
        thread::sleep(Duration::from_millis(500));
    }
}

impl eframe::App for Janela {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let (amostra, historico, nucleos, conectado, custo_janela) = {
            let e = self.estado.lock().unwrap();
            (
                e.atual.clone(),
                e.historico_cpu.iter().copied().collect::<Vec<f32>>(),
                e.nucleos.clone(),
                e.conectado,
                e.custo_janela,
            )
        };
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().show(ui, |ui| {
            let Some(a) = &amostra else {
                ui.label(if conectado {
                    "Coletando a primeira amostra…"
                } else {
                    "Iniciando o coletor… Se esta mensagem não sair, feche e abra o Prumo de novo."
                });
                return;
            };
            if topo(ui, a, custo_janela, conectado) {
                self.config_aberta = !self.config_aberta;
            }
            ui.add_space(8.0);
            // A página rola quando as seções e o mínimo da lista não cabem na janela.
            let altura_do_painel = ui.available_height();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let inicio = ui.cursor().top();
                let mut disco_e_rede_visivel = false;
                for (secao, visivel) in self.config.secoes.clone() {
                    if !visivel {
                        continue;
                    }
                    match secao {
                        Secao::Resumo => resumo(ui, a),
                        Secao::Memoria => memoria(ui, a),
                        Secao::Nucleos => nucleos_fisicos(ui, a, &nucleos),
                        Secao::LinhaDoTempo => linha_do_tempo(ui, &historico),
                        // Não entra na ordem: fica ao lado da lista de processos.
                        Secao::DiscoERede => {
                            disco_e_rede_visivel = true;
                            continue;
                        }
                    }
                    ui.add_space(12.0);
                }

                // Disco e rede ficam à direita da lista de processos. Sem a
                // lista, ou em janela estreita, ocupam a largura toda.
                let ao_lado = disco_e_rede_visivel
                    && self.config.processos
                    && ui.available_width() >= LARGURA_MINIMA_COM_LATERAL;
                if disco_e_rede_visivel && !ao_lado {
                    disco_e_rede(ui, a);
                    ui.add_space(12.0);
                }
                if self.config.processos {
                    let espaco = altura_do_painel - (ui.cursor().top() - inicio);
                    if ao_lado {
                        let largura_dos_processos = ui.available_width() - LARGURA_LATERAL - 20.0;
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(largura_dos_processos);
                                self.processos(ui, a, espaco);
                            });
                            ui.add_space(12.0);
                            ui.vertical(|ui| {
                                ui.set_width(LARGURA_LATERAL);
                                disco(ui, a);
                                ui.add_space(14.0);
                                rede(ui, a);
                            });
                        });
                    } else {
                        self.processos(ui, a, espaco);
                    }
                }
            });
        });
        self.janela_de_config(&ctx);
        self.janela_de_confirmacao(&ctx);
    }
}

fn titulo(ui: &mut Ui, texto: &str, nota: &str, dica: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(texto).strong()).on_hover_text(dica);
        ui.label(RichText::new(nota).small().weak());
    });
    ui.add_space(4.0);
}

/// Devolve se o botão de configuração foi clicado.
fn topo(ui: &mut Ui, a: &Amostra, custo_janela: (f64, f64), conectado: bool) -> bool {
    let mut configurar = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Prumo").size(18.0).strong());
        if conectado {
            ui.label(RichText::new(format!("média de {} s", num(a.decorrido, 1))).small().weak())
                .on_hover_text("Todo valor desta janela é a média do último intervalo de coleta.");
        } else {
            ui.label(RichText::new("sem conexão com o coletor, tentando de novo").small().color(LARANJA));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            configurar = ui.small_button("Configurar").clicked();
            ui.label(
                RichText::new(format!(
                    "custo do Prumo · coletor {}% CPU, {} MB · janela {}% CPU, {} MB",
                    num(a.custo_cpu, 2),
                    num(a.custo_mem_mb, 0),
                    num(custo_janela.0, 2),
                    num(custo_janela.1, 0)
                ))
                .small()
                .weak(),
            )
            .on_hover_text("Quanto o próprio Prumo consome. O coletor fica sempre ligado; a janela só custa enquanto está aberta.");
        });
    });
    configurar
}

fn bloco(ui: &mut Ui, rotulo: &str, valor: String, detalhe: String, dica: &str) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(rotulo).small().weak());
        ui.label(RichText::new(valor).size(22.0).strong());
        ui.label(RichText::new(detalhe).small().weak());
    })
    .response
    .on_hover_text(dica);
}

fn resumo(ui: &mut Ui, a: &Amostra) {
    ui.columns(4, |colunas| {
        bloco(
            &mut colunas[0],
            "CPU · tempo",
            format!("{}%", num(a.cpu_total, 0)),
            format!(
                "{} lógicos · turbo {}",
                a.por_logico.len(),
                a.turbo.map(|t| format!("{}%", num(t, 0))).unwrap_or_else(|| "?".into())
            ),
            DICA_CPU,
        );
        bloco(
            &mut colunas[1],
            "Memória em uso",
            format!("{} GB", gb(a.em_uso)),
            format!("de {} GB", gb(a.total_ram)),
            DICA_MEMORIA_EM_USO,
        );
        match &a.gpu {
            Some((tipo, uso)) => bloco(
                &mut colunas[2],
                &format!("GPU · motor {tipo}"),
                format!("{}%", num(*uso, 0)),
                "o motor mais ocupado".into(),
                DICA_GPU,
            ),
            None => bloco(&mut colunas[2], "GPU", "—".into(), "contadores indisponíveis".into(), DICA_GPU),
        }
        match a.vram_dedicada {
            Some(bytes) => bloco(
                &mut colunas[3],
                "VRAM dedicada",
                format!("{} GB", gb(bytes)),
                "em uso, pelo adaptador".into(),
                DICA_VRAM,
            ),
            None => bloco(&mut colunas[3], "VRAM dedicada", "—".into(), "indisponível".into(), DICA_VRAM),
        }
    });
}

enum Preenchimento {
    Cheio(Color32),
    /// Marca o que é calculado por diferença, não medido.
    Hachurado(Color32),
    Contorno(Color32),
}

fn pintar(ui: &Ui, rect: Rect, preenchimento: &Preenchimento, raio: f32) {
    let p = ui.painter();
    match preenchimento {
        Preenchimento::Cheio(cor) => {
            p.rect_filled(rect, raio, *cor);
        }
        Preenchimento::Hachurado(cor) => {
            p.rect_filled(rect, raio, cor.gamma_multiply(0.25));
            let recortado = p.with_clip_rect(rect);
            let altura = rect.height();
            let mut x = rect.left() - altura;
            while x < rect.right() {
                recortado.line_segment(
                    [Pos2::new(x, rect.bottom()), Pos2::new(x + altura, rect.top())],
                    Stroke::new(1.5, *cor),
                );
                x += 6.0;
            }
        }
        Preenchimento::Contorno(cor) => {
            p.rect_stroke(rect, raio, Stroke::new(1.0, *cor), StrokeKind::Inside);
        }
    }
}

fn memoria(ui: &mut Ui, a: &Amostra) {
    titulo(ui, &format!("Memória · {} GB", gb(a.total_ram)), "as partes somam o total", DICA_MEMORIA);
    let partes: [(&str, f64, Preenchimento, &str); 6] = [
        ("Programas", a.programas, Preenchimento::Cheio(AZUL), DICA_PROGRAMAS),
        ("Windows", a.windows, Preenchimento::Cheio(LARANJA), DICA_WINDOWS),
        (
            "Compartilhado e outros (por diferença)",
            a.compartilhado_e_outros,
            Preenchimento::Hachurado(VERDE),
            DICA_COMPARTILHADO,
        ),
        ("Cache liberável", a.cache, Preenchimento::Cheio(CINZA), DICA_CACHE),
        ("Modificada", a.modificada, Preenchimento::Cheio(CINZA_ESCURO), DICA_MODIFICADA),
        ("Livre", a.livre, Preenchimento::Contorno(CINZA), DICA_LIVRE),
    ];

    let largura = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(largura, 22.0), Sense::hover());
    let mut x = rect.left();
    for (_, valor, preenchimento, _) in &partes {
        let w = (valor / a.total_ram).max(0.0) as f32 * largura;
        if w >= 1.0 {
            let segmento =
                Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new((w - 2.0).max(1.0), rect.height()));
            pintar(ui, segmento, preenchimento, 2.0);
        }
        x += w;
    }

    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for (nome, valor, preenchimento, dica) in &partes {
            let (quadrado, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
            pintar(ui, quadrado, preenchimento, 2.0);
            ui.label(RichText::new(format!("{nome} {}", gb(*valor))).small()).on_hover_text(*dica);
            ui.add_space(8.0);
        }
    });
}

fn nucleos_fisicos(ui: &mut Ui, a: &Amostra, nucleos: &[Vec<usize>]) {
    // Sem a topologia, cada lógico vira um núcleo próprio.
    let reserva: Vec<Vec<usize>>;
    let nucleos = if nucleos.is_empty() {
        reserva = (0..a.por_logico.len()).map(|l| vec![l]).collect();
        &reserva[..]
    } else {
        nucleos
    };
    titulo(
        ui,
        &format!("Núcleos · {} físicos, {} lógicos", nucleos.len(), a.por_logico.len()),
        "50% não é metade da capacidade",
        DICA_NUCLEOS,
    );

    let altura = 44.0;
    let largura = ui.available_width();
    let (rect, resposta) = ui.allocate_exact_size(Vec2::new(largura, altura + 16.0), Sense::hover());
    let p = ui.painter();
    let fundo = ui.visuals().faint_bg_color;
    let fraco = ui.visuals().weak_text_color();
    let passo = largura / nucleos.len().max(1) as f32;
    for (i, logicos) in nucleos.iter().enumerate() {
        let x0 = rect.left() + i as f32 * passo + 4.0;
        let largura_nucleo = passo - 8.0;
        let largura_logico = largura_nucleo / logicos.len().max(1) as f32;
        for (j, logico) in logicos.iter().enumerate() {
            let uso = (a.por_logico.get(*logico).copied().unwrap_or(0.0) / 100.0).clamp(0.0, 1.0) as f32;
            let esquerda = x0 + j as f32 * largura_logico;
            let direita = esquerda + largura_logico - 2.0;
            let base = rect.top() + altura;
            p.rect_filled(
                Rect::from_min_max(Pos2::new(esquerda, rect.top()), Pos2::new(direita, base)),
                2.0,
                fundo,
            );
            p.rect_filled(
                Rect::from_min_max(Pos2::new(esquerda, base - uso * altura), Pos2::new(direita, base)),
                2.0,
                AZUL,
            );
        }
        p.text(
            Pos2::new(x0 + largura_nucleo / 2.0, rect.bottom()),
            Align2::CENTER_BOTTOM,
            i.to_string(),
            FontId::proportional(11.0),
            fraco,
        );
    }
    resposta.on_hover_text(DICA_NUCLEOS);
}

fn linha_do_tempo(ui: &mut Ui, historico: &[f32]) {
    titulo(ui, "Linha do tempo · CPU", "último minuto", DICA_LINHA);
    let altura = 80.0;
    let largura = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(largura, altura), Sense::hover());
    let p = ui.painter();
    let grade = ui.visuals().faint_bg_color;
    let fraco = ui.visuals().weak_text_color();
    for fracao in [0.0, 0.5, 1.0] {
        let y = rect.bottom() - fracao * altura;
        p.hline(rect.left()..=rect.right(), y, Stroke::new(1.0, grade));
    }
    p.text(rect.left_top(), Align2::LEFT_TOP, "100%", FontId::proportional(11.0), fraco);
    p.text(
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_BOTTOM,
        "50%",
        FontId::proportional(11.0),
        fraco,
    );
    if historico.len() < 2 {
        return;
    }
    let passo = largura / (HISTORICO - 1) as f32;
    let n = historico.len();
    let pontos: Vec<Pos2> = historico
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.right() - (n - 1 - i) as f32 * passo;
            let y = rect.bottom() - (v / 100.0).clamp(0.0, 1.0) * altura;
            Pos2::new(x, y)
        })
        .collect();
    p.line(pontos, Stroke::new(2.0, AZUL));
}

/// Barra fina de 0 a 100%.
fn barra(ui: &mut Ui, fracao: f32, largura: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(largura, 8.0), Sense::hover());
    ui.painter().rect_filled(rect, 2.0, ui.visuals().faint_bg_color);
    let cheio = Rect::from_min_size(rect.min, Vec2::new(rect.width() * fracao.clamp(0.0, 1.0), rect.height()));
    ui.painter().rect_filled(cheio, 2.0, AZUL);
}

/// Disco e rede lado a lado, na largura toda. Usado quando não ficam ao lado
/// da lista de processos.
fn disco_e_rede(ui: &mut Ui, a: &Amostra) {
    ui.columns(2, |colunas| {
        disco(&mut colunas[0], a);
        rede(&mut colunas[1], a);
    });
}

/// Cada disco em duas linhas curtas, para caber numa coluna estreita.
fn disco(ui: &mut Ui, a: &Amostra) {
    titulo(ui, "Disco", "atividade e taxa", DICA_DISCO);
    if a.discos.is_empty() {
        ui.label(RichText::new("contadores indisponíveis").small().weak());
    }
    for disco in &a.discos {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&disco.nome).small());
            barra(ui, disco.atividade as f32 / 100.0, 70.0);
            ui.label(RichText::new(format!("{}%", num(disco.atividade, 0))).small());
        });
        ui.label(
            RichText::new(format!("lê {} · escreve {}", taxa(disco.leitura), taxa(disco.escrita))).small().weak(),
        );
        ui.add_space(2.0);
    }
}

fn rede(ui: &mut Ui, a: &Amostra) {
    titulo(ui, "Rede", "por interface", DICA_REDE);
    let mut ativas: Vec<_> = a.redes.iter().filter(|r| r.recebido + r.enviado >= 512.0).collect();
    ativas.sort_by(|x, y| {
        (y.recebido + y.enviado).partial_cmp(&(x.recebido + x.enviado)).unwrap_or(std::cmp::Ordering::Equal)
    });
    if ativas.is_empty() {
        ui.label(RichText::new(format!("sem tráfego agora · {} interfaces", a.redes.len())).small().weak());
    }
    for rede in ativas.iter().take(3) {
        ui.label(RichText::new(format!("recebe {}", taxa(rede.recebido))).small());
        ui.label(RichText::new(format!("envia {}", taxa(rede.enviado))).small());
        ui.add(egui::Label::new(RichText::new(&rede.nome).small().weak()).truncate());
        ui.add_space(2.0);
    }
}

// ---------------------------------------------------------------------------
// Lista de processos
// ---------------------------------------------------------------------------

/// Seta de ordenação desenhada, porque a fonte padrão não tem o caractere.
fn seta(ui: &Ui, centro: Pos2, para_baixo: bool, cor: Color32) {
    let s = 4.0;
    let pontos = if para_baixo {
        vec![centro + Vec2::new(-s, -s * 0.6), centro + Vec2::new(s, -s * 0.6), centro + Vec2::new(0.0, s * 0.7)]
    } else {
        vec![centro + Vec2::new(-s, s * 0.6), centro + Vec2::new(s, s * 0.6), centro + Vec2::new(0.0, -s * 0.7)]
    };
    ui.painter().add(Shape::convex_polygon(pontos, cor, Stroke::NONE));
}

/// Marca de gaveta: aponta para a direita quando fechada, para baixo quando aberta.
fn marca_de_gaveta(ui: &Ui, centro: Pos2, aberta: bool, cor: Color32) {
    if aberta {
        seta(ui, centro, true, cor);
    } else {
        let s = 4.0;
        let pontos = vec![
            centro + Vec2::new(-s * 0.6, -s),
            centro + Vec2::new(-s * 0.6, s),
            centro + Vec2::new(s * 0.7, 0.0),
        ];
        ui.painter().add(Shape::convex_polygon(pontos, cor, Stroke::NONE));
    }
}

/// Uma linha da tabela, pintada à mão para alinhar os números à direita.
/// `gaveta` é `Some(aberta)` nas linhas que abrem detalhes.
fn linha(
    ui: &mut Ui,
    larguras: &[f32],
    textos: &[String],
    cor: Color32,
    clicavel: bool,
    gaveta: Option<bool>,
) -> Response {
    let total: f32 = larguras.iter().sum();
    let sentido = if clicavel { Sense::click() } else { Sense::hover() };
    let (rect, resposta) = ui.allocate_exact_size(Vec2::new(total, ALTURA_LINHA), sentido);
    if clicavel && (resposta.hovered() || gaveta == Some(true)) {
        ui.painter().rect_filled(rect, 2.0, ui.visuals().faint_bg_color);
    }
    let fonte = FontId::proportional(14.0);
    let mut x = rect.left();
    for (i, texto) in textos.iter().enumerate() {
        let celula = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(larguras[i], ALTURA_LINHA));
        // Nas linhas de soma, o rótulo pode avançar sobre a folga da coluna
        // seguinte em vez de ser cortado.
        let recorte = if i == 0 && !clicavel { celula.expand2(Vec2::new(30.0, 0.0)) } else { celula };
        let pintor = ui.painter().with_clip_rect(recorte);
        if i == 0 {
            if let Some(aberta) = gaveta {
                marca_de_gaveta(ui, Pos2::new(celula.left() + 9.0, celula.center().y), aberta, ui.visuals().weak_text_color());
            }
            pintor.text(celula.left_center() + Vec2::new(RECUO_DO_NOME, 0.0), Align2::LEFT_CENTER, texto, fonte.clone(), cor);
        } else {
            pintor.text(
                celula.right_center() - Vec2::new(6.0, 0.0),
                Align2::RIGHT_CENTER,
                texto,
                fonte.clone(),
                cor,
            );
        }
        x += larguras[i];
    }
    resposta
}

fn vram_ou_traco(bytes: f64) -> String {
    if bytes >= 1024.0 * 1024.0 {
        tamanho(bytes)
    } else {
        "—".into()
    }
}

fn rotulo_do_grupo(g: &GrupoDeProcessos) -> String {
    if g.processos() > 1 {
        format!("{} ({})", g.nome, g.processos())
    } else {
        g.nome.clone()
    }
}

fn gpu_ou_traco(gpu: &Option<(String, f64)>) -> String {
    gpu.as_ref().map(|(tipo, uso)| format!("{tipo} {}%", num(*uso, 1))).unwrap_or_else(|| "—".into())
}

fn textos_do_grupo(g: &GrupoDeProcessos, colunas: &[Coluna]) -> Vec<String> {
    let mut textos = vec![rotulo_do_grupo(g)];
    for coluna in colunas {
        textos.push(match coluna {
            Coluna::Cpu => format!("{}%", num(g.cpu, 1)),
            Coluna::Privada => tamanho(g.privada),
            Coluna::Compartilhado => tamanho(g.working_set),
            Coluna::Es => taxa(g.es()),
            Coluna::Gpu => gpu_ou_traco(&g.gpu),
            Coluna::Vram => vram_ou_traco(g.vram),
        });
    }
    textos
}

/// O que o usuário escolheu no menu de controle.
enum Escolha {
    Prioridade(Prioridade),
    /// Pede confirmação antes de encerrar.
    Encerrar,
}

/// O nível mais restritivo entre os processos do grupo.
fn protecao_do_grupo(g: &GrupoDeProcessos) -> Protecao {
    g.membros
        .iter()
        .map(|m| controle::protecao(&g.nome, &m.caminho))
        .fold(Protecao::Livre, |maior, p| if p > maior { p } else { maior })
}

/// Menu de controle de um grupo. Devolve a escolha, se houver.
fn menu_de_controle(ui: &mut Ui, g: &GrupoDeProcessos, liberar_windows: bool) -> Option<Escolha> {
    ui.label(RichText::new(rotulo_do_grupo(g)).strong());
    let protecao = protecao_do_grupo(g);
    if !controle::permitido(protecao, liberar_windows) {
        let motivo = if protecao == Protecao::Critico {
            "Essencial para o Windows: o Prumo não altera nem encerra."
        } else {
            "Processo do Windows: bloqueado.\nPara liberar, marque a opção em Configurar."
        };
        ui.label(RichText::new(motivo).small().weak());
        return None;
    }
    // Mesmo liberado pelo Prumo, o Windows pode não deixar: é o caso de
    // serviços e de processos de outra conta. Melhor explicar do que oferecer
    // uma ação que vai ser recusada.
    if !g.membros.iter().any(|m| controle::acessivel(m.pid)) {
        let motivo = match g.membros.iter().flat_map(|m| m.servicos.iter()).next() {
            Some(servico) => format!(
                "Roda como serviço do Windows: {}.\nConta: {}.\n\nSem administrador, o Windows não deixa mudar a prioridade nem encerrar.\nPara parar ou desativar, use o gerenciador de serviços (services.msc).",
                if servico.exibicao.is_empty() { &servico.nome } else { &servico.exibicao },
                if servico.conta.is_empty() { "não informada" } else { &servico.conta }
            ),
            None => "Pertence a outra conta ou é protegido.\nSem administrador, o Windows não deixa mudar a prioridade nem encerrar.".to_string(),
        };
        ui.label(RichText::new(motivo).small().weak());
        return None;
    }
    if protecao == Protecao::Windows {
        ui.label(RichText::new("Processo do Windows, liberado na configuração.").small().color(LARANJA));
    }
    let atual = g.membros.first().and_then(|m| controle::prioridade_atual(m.pid));
    ui.label(
        RichText::new(format!(
            "Prioridade atual: {}",
            atual.map(|p| p.nome()).unwrap_or("o Windows não deixou consultar")
        ))
        .small()
        .weak(),
    );
    ui.separator();
    let mut escolhida = None;
    for prioridade in PRIORIDADES {
        if ui.selectable_label(atual == Some(prioridade), prioridade.nome()).clicked() {
            escolhida = Some(Escolha::Prioridade(prioridade));
            ui.close();
        }
    }
    ui.label(RichText::new("Vale até o programa fechar.").small().weak());
    ui.label(RichText::new("Alta pode deixar o resto do sistema lento.").small().weak());
    ui.separator();
    let rotulo = if g.processos() > 1 {
        format!("Encerrar os {} processos…", g.processos())
    } else {
        "Encerrar…".to_string()
    };
    if ui
        .button(RichText::new(rotulo).color(VERMELHO))
        .on_hover_text("Fecha o programa na hora, sem dar chance de salvar. Pede confirmação antes.")
        .clicked()
    {
        escolhida = Some(Escolha::Encerrar);
        ui.close();
    }
    escolhida
}

/// Pasta de um caminho de arquivo, para usar como filtro.
fn pasta_de(caminho: &str) -> &str {
    caminho.rsplit_once('\\').map(|(pasta, _)| pasta).unwrap_or(caminho)
}

impl Janela {
    fn cabecalho(&mut self, ui: &mut Ui, larguras: &[f32], colunas: &[Coluna]) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for i in 0..larguras.len() {
                let (rotulo, ordem, dica) = if i == 0 {
                    ("Processo", Ordem::Nome, DICA_PROCESSOS)
                } else {
                    let coluna = colunas[i - 1];
                    (coluna.nome(), Ordem::Por(coluna), dica_da_coluna(coluna))
                };
                let (rect, resposta) =
                    ui.allocate_exact_size(Vec2::new(larguras[i], ALTURA_LINHA), Sense::click());
                let ativa = self.ordem == ordem;
                let cor = if ativa || resposta.hovered() {
                    ui.visuals().strong_text_color()
                } else {
                    ui.visuals().weak_text_color()
                };
                let fonte = FontId::proportional(13.0);
                if i == 0 {
                    let texto = ui.painter().text(
                        rect.left_center() + Vec2::new(RECUO_DO_NOME, 0.0),
                        Align2::LEFT_CENTER,
                        rotulo,
                        fonte,
                        cor,
                    );
                    if ativa {
                        seta(ui, Pos2::new(texto.right() + 10.0, rect.center().y), self.decrescente, cor);
                    }
                } else {
                    let recuo = if ativa { 20.0 } else { 6.0 };
                    ui.painter().text(
                        rect.right_center() - Vec2::new(recuo, 0.0),
                        Align2::RIGHT_CENTER,
                        rotulo,
                        fonte,
                        cor,
                    );
                    if ativa {
                        seta(ui, Pos2::new(rect.right() - 10.0, rect.center().y), self.decrescente, cor);
                    }
                }
                if resposta.clicked() {
                    if ativa {
                        self.decrescente = !self.decrescente;
                    } else {
                        self.ordem = ordem;
                        // Nome começa de A a Z; números, do maior para o menor.
                        self.decrescente = ordem != Ordem::Nome;
                    }
                }
                resposta.on_hover_text(dica);
            }
        });
    }

    /// `espaco` é a altura que sobra na janela para esta seção.
    fn processos(&mut self, ui: &mut Ui, a: &Amostra, espaco: f32) {
        let inicio = ui.cursor().top();
        titulo(
            ui,
            &format!("Processos · {}", a.n_processos),
            "clique abre os detalhes na própria linha · botão direito para controle · cabeçalho ordena",
            DICA_PROCESSOS,
        );

        let colunas = self.config.colunas_visiveis();
        if let Ordem::Por(coluna) = self.ordem {
            if !colunas.contains(&coluna) {
                self.ordem = Ordem::Peso;
                self.decrescente = true;
            }
        }

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filtro)
                    .hint_text("Filtrar por nome, caminho, janela ou projeto")
                    .desired_width(300.0),
            )
            .on_hover_text("Mostra só os programas em que o texto aparece no nome, no caminho do executável, no título de uma janela ou na linha de comando.\n\nExemplo: o nome de um projeto encontra o editor que está com ele aberto.");
            if !self.filtro.is_empty() && ui.small_button("Limpar").clicked() {
                self.filtro.clear();
            }
            if self.ordem != Ordem::Peso
                && ui
                    .small_button("Ordenar por peso")
                    .on_hover_text("Ordem padrão: do mais pesado para o mais leve, pela maior entre a fatia de CPU e a fatia de memória.")
                    .clicked()
            {
                self.ordem = Ordem::Peso;
                self.decrescente = true;
            }
        });
        ui.add_space(4.0);

        // ---- Filtro e ordenação ----
        let filtro = self.filtro.trim().to_lowercase();
        let filtrando = !filtro.is_empty();
        let mut visiveis: Vec<&GrupoDeProcessos> = a
            .grupos
            .iter()
            .filter(|g| {
                !filtrando
                    || g.nome.to_lowercase().contains(&filtro)
                    || g.membros.iter().any(|m| m.contem(&filtro))
            })
            .collect();
        match self.ordem {
            Ordem::Peso => {} // já vem do coletor, do mais pesado para o mais leve
            Ordem::Nome => visiveis.sort_by_key(|g| std::cmp::Reverse(g.nome.to_lowercase())),
            Ordem::Por(coluna) => {
                let chave = |g: &GrupoDeProcessos| -> f64 {
                    match coluna {
                        Coluna::Cpu => g.cpu,
                        Coluna::Privada => g.privada,
                        Coluna::Compartilhado => g.working_set,
                        Coluna::Es => g.es(),
                        Coluna::Gpu => g.gpu.as_ref().map(|(_, uso)| *uso).unwrap_or(-1.0),
                        Coluna::Vram => g.vram,
                    }
                };
                visiveis.sort_by(|x, y| chave(y).partial_cmp(&chave(x)).unwrap_or(std::cmp::Ordering::Equal));
            }
        }
        if !self.decrescente {
            visiveis.reverse();
        }
        // A ordenação escolhida é lembrada na configuração.
        if self.config.ordem != chave_da_ordem(self.ordem) || self.config.decrescente != self.decrescente {
            self.config.ordem = chave_da_ordem(self.ordem).to_string();
            self.config.decrescente = self.decrescente;
            self.config.salvar();
        }

        // ---- Tabela ----
        let numericas: f32 = colunas.iter().map(|c| c.largura()).sum();
        let mut larguras = vec![(ui.available_width() - 16.0 - numericas).max(160.0)];
        larguras.extend(colunas.iter().map(|c| c.largura()));
        self.cabecalho(ui, &larguras, &colunas);

        let texto = ui.visuals().text_color();
        let fraco = ui.visuals().weak_text_color();
        let forte = ui.visuals().strong_text_color();

        // A lista ocupa o que sobrar da janela, menos o rodapé, com um mínimo
        // de linhas; abaixo disso é a página que rola.
        let rodape = ALTURA_LINHA * 2.0 + 56.0;
        let usado = ui.cursor().top() - inicio;
        let altura_da_lista = (espaco - usado - rodape).max(ALTURA_LINHA * LINHAS_MINIMAS);
        let mut acao: Option<(String, Vec<u32>, Escolha)> = None;
        let mut encerrar: Option<(String, Vec<u32>)> = None;
        let liberar_windows = self.config.liberar_windows;
        let mut alternar: Option<String> = None;
        let mut novo_filtro: Option<String> = None;
        let largura_total: f32 = larguras.iter().sum();
        egui::ScrollArea::vertical().max_height(altura_da_lista).auto_shrink([false, false]).show(ui, |ui| {
            for g in &visiveis {
                let aberta = self.expandidos.contains(&g.nome);
                // Identidade pelo nome: menu e gaveta não trocam de programa quando a ordem muda.
                let resposta = ui
                    .push_id(&g.nome, |ui| {
                        linha(ui, &larguras, &textos_do_grupo(g, &colunas), texto, true, Some(aberta))
                    })
                    .inner;
                if resposta.clicked() {
                    alternar = Some(g.nome.clone());
                }
                resposta.context_menu(|ui| {
                    if let Some(escolha) = menu_de_controle(ui, g, liberar_windows) {
                        acao = Some((rotulo_do_grupo(g), g.pids(), escolha));
                    }
                });
                if aberta {
                    ui.push_id((&g.nome, "gaveta"), |ui| {
                        let pode_encerrar = controle::permitido(protecao_do_grupo(g), liberar_windows);
                        Janela::gaveta_de_detalhes(ui, g, largura_total, pode_encerrar, &mut novo_filtro, &mut encerrar)
                    });
                }
            }
        });
        if let Some(nome) = alternar {
            if !self.expandidos.remove(&nome) {
                self.expandidos.insert(nome);
            }
        }
        if let Some(filtro) = novo_filtro {
            self.filtro = filtro;
        }
        // Programa que encerrou não fica guardado como aberto.
        self.expandidos.retain(|nome| a.grupos.iter().any(|g| &g.nome == nome));

        if encerrar.is_some() {
            self.confirmar = encerrar;
        }
        if let Some((nome, pids, Escolha::Encerrar)) = &acao {
            self.confirmar = Some((nome.clone(), pids.clone()));
        }
        if let Some((nome, pids, Escolha::Prioridade(prioridade))) = acao {
            let (aplicados, recusados) = controle::definir_prioridade(&pids, prioridade);
            self.aviso = Some(if recusados == 0 {
                format!("Prioridade de {nome}: {}.", prioridade.nome())
            } else {
                format!(
                    "Prioridade de {nome}: {} em {aplicados} de {} processos; o Windows recusou {recusados}.",
                    prioridade.nome(),
                    pids.len()
                )
            });
        }

        // ---- Rodapé: as somas ----
        ui.separator();
        if filtrando {
            let soma = |f: fn(&GrupoDeProcessos) -> f64| -> f64 { visiveis.iter().map(|g| f(g)).sum() };
            let mut textos = vec![format!("Soma do filtro ({} de {} programas)", visiveis.len(), a.grupos.len())];
            for coluna in &colunas {
                textos.push(match coluna {
                    Coluna::Cpu => format!("{}%", num(soma(|g| g.cpu), 1)),
                    Coluna::Privada => tamanho(soma(|g| g.privada)),
                    Coluna::Compartilhado => "não somável".into(),
                    Coluna::Es => taxa(soma(|g| g.es())),
                    Coluna::Gpu => String::new(),
                    Coluna::Vram => vram_ou_traco(soma(|g| g.vram)),
                });
            }
            linha(ui, &larguras, &textos, forte, false, None);
        } else {
            let mut textos = vec!["Vida curta e não atribuído".to_string()];
            for coluna in &colunas {
                textos.push(match coluna {
                    Coluna::Cpu => format!("{}%", num(a.cpu_nao_atribuida(), 1)),
                    _ => "—".into(),
                });
            }
            linha(ui, &larguras, &textos, fraco, false, None).on_hover_text(
                "CPU que a máquina gastou e que nenhum processo visto explica: processos que nasceram e morreram dentro do intervalo, e pequenas defasagens entre os contadores.",
            );

            let mut textos = vec![format!("Soma ({} programas)", a.grupos.len())];
            for coluna in &colunas {
                textos.push(match coluna {
                    Coluna::Cpu => format!("{}%", num(a.cpu_total, 1)),
                    Coluna::Privada => tamanho(a.programas),
                    Coluna::Compartilhado => "não somável".into(),
                    Coluna::Es => taxa(a.grupos.iter().map(|g| g.es()).sum()),
                    Coluna::Gpu => String::new(),
                    Coluna::Vram => tamanho(a.vram_processos),
                });
            }
            linha(ui, &larguras, &textos, forte, false, None);
        }

        if let Some(aviso) = &self.aviso {
            ui.label(RichText::new(aviso).small());
        } else if let (Some(dedicada), false) = (a.vram_dedicada, filtrando) {
            ui.label(
                RichText::new(format!(
                    "VRAM: os processos somam {} GB e o adaptador reporta {} GB. A diferença é memória contada em mais de um processo.",
                    gb(a.vram_processos),
                    gb(dedicada)
                ))
                .small()
                .weak(),
            );
        }
    }

    /// Detalhes de um programa, abertos logo abaixo da linha dele na lista.
    fn gaveta_de_detalhes(
        ui: &mut Ui,
        g: &GrupoDeProcessos,
        largura: f32,
        pode_encerrar: bool,
        novo_filtro: &mut Option<String>,
        encerrar: &mut Option<(String, Vec<u32>)>,
    ) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(largura - 14.0);
            ui.label(
                RichText::new(format!(
                    "CPU {}% · privada {} · com compartilhado {} · lê {} · escreve {} · GPU {} · VRAM {}",
                    num(g.cpu, 1),
                    tamanho(g.privada),
                    tamanho(g.working_set),
                    taxa(g.es_leitura),
                    taxa(g.es_escrita),
                    gpu_ou_traco(&g.gpu),
                    vram_ou_traco(g.vram)
                ))
                .small(),
            );
            ui.add_space(6.0);

            // ---- Executáveis ----
            ui.label(RichText::new("Executável").strong());
            let mut caminhos: Vec<&str> =
                g.membros.iter().map(|m| m.caminho.as_str()).filter(|c| !c.is_empty()).collect();
            caminhos.sort_unstable();
            caminhos.dedup();
            if caminhos.is_empty() {
                ui.label(RichText::new("O Windows não deixou consultar o caminho deste programa.").small().weak());
            }
            for caminho in caminhos {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(caminho).monospace().small());
                    if ui.small_button("Copiar").clicked() {
                        ui.ctx().copy_text(caminho.to_string());
                    }
                    if ui
                        .small_button("Filtrar pela pasta")
                        .on_hover_text("Mostra na lista todos os programas que rodam a partir desta pasta.")
                        .clicked()
                    {
                        *novo_filtro = Some(pasta_de(caminho).to_string());
                    }
                });
            }
            // ---- Serviços ----
            let mut servicos: Vec<_> = g.membros.iter().flat_map(|m| m.servicos.iter()).collect();
            servicos.sort_by(|x, y| x.nome.cmp(&y.nome));
            servicos.dedup_by(|x, y| x.nome == y.nome);
            if !servicos.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(format!("Serviços do Windows · {}", servicos.len())).strong()).on_hover_text(
                    "Este programa roda como serviço: o Windows o inicia e o mantém, mesmo sem ninguém logado.\n\nEncerrar o processo não resolve, porque o serviço costuma voltar. Para parar ou desativar, use o gerenciador de serviços (services.msc), como administrador.",
                );
                for servico in servicos.iter().take(SERVICOS_NA_GAVETA) {
                    let nome = if servico.exibicao.is_empty() || servico.exibicao == servico.nome {
                        servico.nome.clone()
                    } else {
                        format!("{} ({})", servico.exibicao, servico.nome)
                    };
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!(
                                "{nome} · início {} · conta {}",
                                if servico.inicio.is_empty() { "não informado" } else { &servico.inicio },
                                if servico.conta.is_empty() { "não informada" } else { &servico.conta }
                            ))
                            .small(),
                        )
                        .truncate(),
                    );
                }
                if servicos.len() > SERVICOS_NA_GAVETA {
                    ui.label(
                        RichText::new(format!("e mais {} serviços", servicos.len() - SERVICOS_NA_GAVETA)).small().weak(),
                    );
                }
            }

            // ---- Janelas e linhas de comando ----
            let mut titulos: Vec<&str> =
                g.membros.iter().flat_map(|m| m.titulos.iter().map(|t| t.as_str())).collect();
            titulos.sort_unstable();
            titulos.dedup();
            if !titulos.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("Janelas").strong());
                for titulo in titulos.iter().take(6) {
                    ui.add(egui::Label::new(RichText::new(*titulo).small()).truncate());
                }
            }

            let mut comandos: Vec<&str> =
                g.membros.iter().map(|m| m.comando.as_str()).filter(|c| !c.is_empty()).collect();
            comandos.sort_unstable();
            comandos.dedup();
            if !comandos.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("Linha de comando").strong())
                    .on_hover_text("Como cada processo foi iniciado. Costuma trazer o projeto ou arquivo aberto. Passe o mouse numa linha para ver inteira.");
                for comando in comandos.iter().take(COMANDOS_NA_GAVETA) {
                    ui.add(egui::Label::new(RichText::new(*comando).monospace().small()).truncate())
                        .on_hover_text(*comando);
                }
                if comandos.len() > COMANDOS_NA_GAVETA {
                    ui.label(
                        RichText::new(format!("e mais {} linhas diferentes", comandos.len() - COMANDOS_NA_GAVETA))
                            .small()
                            .weak(),
                    );
                }
            }
            ui.add_space(6.0);

            // ---- Processos do grupo ----
            ui.label(RichText::new(format!("Processos · {}", g.processos())).strong());
            egui::Grid::new("membros").num_columns(9).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                for cabecalho in ["PID", "CPU", "Privada", "Com compart.", "Lê", "Escreve", "VRAM", "Prioridade", ""] {
                    ui.label(RichText::new(cabecalho).small().weak());
                }
                ui.end_row();
                // Os membros vêm do maior para o menor em memória privada.
                for m in g.membros.iter().take(MEMBROS_NA_GAVETA) {
                    ui.label(m.pid.to_string());
                    ui.label(format!("{}%", num(m.cpu, 1)));
                    ui.label(tamanho(m.privada));
                    ui.label(tamanho(m.working_set));
                    ui.label(taxa(m.es_leitura));
                    ui.label(taxa(m.es_escrita));
                    ui.label(vram_ou_traco(m.vram));
                    ui.label(controle::prioridade_atual(m.pid).map(|p| p.nome()).unwrap_or("—"));
                    if !controle::acessivel(m.pid) {
                        ui.label(RichText::new("sem permissão").small().weak())
                            .on_hover_text("O Windows não deixa este usuário mexer neste processo. Só com administrador.");
                    } else if pode_encerrar {
                        if ui
                            .small_button(RichText::new("Encerrar…").color(VERMELHO))
                            .on_hover_text("Fecha só este processo, sem chance de salvar. Pede confirmação antes.")
                            .clicked()
                        {
                            *encerrar = Some((format!("{} (processo {})", g.nome, m.pid), vec![m.pid]));
                        }
                    } else {
                        ui.label(RichText::new("bloqueado").small().weak());
                    }
                    ui.end_row();
                }
            });
            if g.membros.len() > MEMBROS_NA_GAVETA {
                let resto = &g.membros[MEMBROS_NA_GAVETA..];
                ui.label(
                    RichText::new(format!(
                        "e mais {} processos menores, somando {} de memória privada",
                        resto.len(),
                        tamanho(resto.iter().map(|m| m.privada).sum())
                    ))
                    .small()
                    .weak(),
                );
            }
        });
    }

    /// Pergunta antes de encerrar, porque não tem volta.
    fn janela_de_confirmacao(&mut self, ctx: &egui::Context) {
        let Some((nome, pids)) = self.confirmar.clone() else { return };
        let mut decisao: Option<bool> = None;
        egui::Window::new("Encerrar programa?")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(&nome).strong());
                ui.label(if pids.len() > 1 {
                    format!("Os {} processos fecham na hora.", pids.len())
                } else {
                    "O processo fecha na hora.".to_string()
                });
                ui.label("O que não foi salvo nele será perdido. Não dá para desfazer.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancelar").clicked() {
                        decisao = Some(false);
                    }
                    if ui.button(RichText::new("Encerrar").color(VERMELHO)).clicked() {
                        decisao = Some(true);
                    }
                });
            });
        match decisao {
            Some(true) => {
                let (encerrados, recusados) = controle::encerrar(&pids);
                self.aviso = Some(if recusados == 0 {
                    format!("{nome}: encerrado.")
                } else {
                    format!("{nome}: {encerrados} de {} encerrados; o Windows recusou {recusados}.", pids.len())
                });
                self.confirmar = None;
            }
            Some(false) => self.confirmar = None,
            None => {}
        }
    }

    fn janela_de_config(&mut self, ctx: &egui::Context) {
        if !self.config_aberta {
            return;
        }
        let antes = self.config.clone();
        let mut aberta = true;
        egui::Window::new("Configuração").open(&mut aberta).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new("Seções").strong());
            ui.label(RichText::new("Marque o que aparece e use os botões para mudar a ordem.").small().weak());
            let mut mover: Option<(usize, usize)> = None;
            // Disco e rede não entram na ordem: têm lugar fixo ao lado dos processos.
            let ordenaveis: Vec<usize> = (0..self.config.secoes.len())
                .filter(|i| self.config.secoes[*i].0 != Secao::DiscoERede)
                .collect();
            let n = ordenaveis.len();
            for (posicao, &i) in ordenaveis.iter().enumerate() {
                ui.horizontal(|ui| {
                    let (secao, visivel) = &mut self.config.secoes[i];
                    ui.checkbox(visivel, secao.nome());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(posicao + 1 < n, egui::Button::new("Descer").small()).clicked() {
                            mover = Some((i, ordenaveis[posicao + 1]));
                        }
                        if ui.add_enabled(posicao > 0, egui::Button::new("Subir").small()).clicked() {
                            mover = Some((i, ordenaveis[posicao - 1]));
                        }
                    });
                });
            }
            if let Some((de, para)) = mover {
                self.config.secoes.swap(de, para);
            }
            ui.checkbox(&mut self.config.processos, "Processos");
            ui.label(
                RichText::new("A lista de processos fica sempre por último e ocupa o espaço que sobrar.")
                    .small()
                    .weak(),
            );
            if let Some(i) = self.config.secoes.iter().position(|(secao, _)| *secao == Secao::DiscoERede) {
                ui.checkbox(&mut self.config.secoes[i].1, "Disco e rede");
                ui.label(
                    RichText::new("Ficam à direita da lista de processos. Em janela estreita, ou sem a lista, ocupam a largura toda.")
                        .small()
                        .weak(),
                );
            }

            ui.separator();
            ui.label(RichText::new("Colunas da lista de processos").strong());
            for (i, coluna) in Coluna::TODAS.iter().enumerate() {
                ui.checkbox(&mut self.config.colunas[i], coluna.nome()).on_hover_text(dica_da_coluna(*coluna));
            }

            ui.separator();
            ui.label(RichText::new("Controle").strong());
            ui.checkbox(&mut self.config.liberar_windows, "Liberar processos do Windows")
                .on_hover_text("Permite mudar a prioridade e encerrar processos que rodam da pasta do Windows, como o Explorer e os serviços.\n\nEncerrar um deles pode derrubar recursos do sistema até reiniciar. Os processos essenciais continuam bloqueados mesmo com isto marcado.");
            if self.config.liberar_windows {
                ui.label(
                    RichText::new("Encerrar um processo do Windows pode tirar recursos do ar até você reiniciar.")
                        .small()
                        .color(LARANJA),
                );
            } else {
                ui.label(
                    RichText::new("Desmarcado, o Prumo só mexe em programas comuns. Os essenciais ficam bloqueados sempre.")
                        .small()
                        .weak(),
                );
            }

            ui.separator();
            if ui.button("Restaurar padrão").clicked() {
                self.config = Config::default();
            }
            ui.label(RichText::new("Salvo em %APPDATA%\\Prumo\\config.txt").small().weak());
        });
        if self.config != antes {
            // "Restaurar padrão" também volta a ordenação.
            self.ordem = ordem_da_config(&self.config);
            self.decrescente = self.config.decrescente;
            self.config.salvar();
        }
        self.config_aberta = aberta;
    }
}
