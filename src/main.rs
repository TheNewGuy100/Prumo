//! Prumo: monitor de recursos para Windows em que os números fecham.
//!
//! Modos:
//!   prumo                      abre a janela (e inicia o coletor, se preciso)
//!   prumo --abrir NOME         o mesmo, já com os detalhes do programa abertos
//!   prumo --filtro TEXTO       o mesmo, já com a lista filtrada
//!   prumo --coletor            roda só o coletor, em segundo plano
//!   prumo --parar              encerra o coletor em execução
//!   prumo --terminal           imprime as amostras no terminal
//!   prumo --amostras N         o mesmo, limitado a N amostras
//!   prumo --prioridade PID P   muda a prioridade de um processo
//!                              (P: baixa, abaixo, normal, acima, alta)
//!   prumo --encerrar PID       encerra um processo, sem chance de salvar
//!
//! Prioridade e encerramento respeitam as travas de `controle.rs`: processos
//! essenciais nunca, processos do Windows só com a opção liberada.
//!
//! `--intervalo-ms N` muda o intervalo de coleta (padrão 1000, mínimo 100).

mod coletor;
mod config;
mod controle;
mod janela;
mod protocolo;
mod servico;
mod texto;

use std::thread;
use std::time::Duration;

use coletor::{Amostra, Coletor};
use controle::Prioridade;
use texto::{gb, num};

enum Modo {
    Janela,
    Coletor { sair_sem_clientes: bool },
    Parar,
    Terminal { amostras: u64 },
    Prioridade { pid: u32, prioridade: Prioridade },
    Encerrar { pid: u32 },
}

/// Aplica à linha de comando as mesmas travas da janela. Devolve o motivo da recusa.
fn checar_protecao(pid: u32) -> Result<(), String> {
    let (caminho, _) = coletor::identidade_do_processo(pid);
    let nome = std::path::Path::new(&caminho).file_stem().and_then(|n| n.to_str()).unwrap_or("").to_string();
    let protecao = controle::protecao(&nome, &caminho);
    if controle::permitido(protecao, config::Config::carregar().liberar_windows) {
        return Ok(());
    }
    Err(match protecao {
        controle::Protecao::Critico => format!("O processo {pid} é essencial para o Windows; o Prumo não mexe nele."),
        _ => format!(
            "O processo {pid} é do Windows e está bloqueado. Para liberar, marque a opção em Configurar, na janela do Prumo."
        ),
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut modo = Modo::Janela;
    let mut intervalo_ms: u64 = 1000;
    let mut abertos: Vec<String> = Vec::new();
    let mut filtro = String::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--filtro" if i + 1 < args.len() => {
                filtro = args[i + 1].clone();
                i += 1;
            }
            "--abrir" if i + 1 < args.len() => {
                abertos.push(args[i + 1].clone());
                i += 1;
            }
            "--coletor" => modo = Modo::Coletor { sair_sem_clientes: false },
            "--sair-sem-clientes" => {
                if let Modo::Coletor { sair_sem_clientes } = &mut modo {
                    *sair_sem_clientes = true;
                }
            }
            "--parar" => modo = Modo::Parar,
            "--terminal" => modo = Modo::Terminal { amostras: u64::MAX },
            "--amostras" if i + 1 < args.len() => {
                modo = Modo::Terminal { amostras: args[i + 1].parse().unwrap_or(u64::MAX) };
                i += 1;
            }
            "--prioridade" if i + 2 < args.len() => {
                match (args[i + 1].parse(), Prioridade::do_argumento(&args[i + 2])) {
                    (Ok(pid), Some(prioridade)) => modo = Modo::Prioridade { pid, prioridade },
                    _ => {
                        eprintln!("Uso: prumo --prioridade PID baixa|abaixo|normal|acima|alta");
                        std::process::exit(2);
                    }
                }
                i += 2;
            }
            "--encerrar" if i + 1 < args.len() => {
                match args[i + 1].parse() {
                    Ok(pid) => modo = Modo::Encerrar { pid },
                    Err(_) => {
                        eprintln!("Uso: prumo --encerrar PID");
                        std::process::exit(2);
                    }
                }
                i += 1;
            }
            "--intervalo-ms" if i + 1 < args.len() => {
                intervalo_ms = args[i + 1].parse().unwrap_or(1000).max(100);
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    match modo {
        Modo::Janela => {
            if let Err(erro) = janela::abrir(intervalo_ms, abertos, filtro) {
                eprintln!("Não foi possível abrir a janela: {erro}");
                std::process::exit(1);
            }
        }
        Modo::Coletor { sair_sem_clientes } => servico::rodar(intervalo_ms, sair_sem_clientes),
        Modo::Parar => {
            if servico::parar() {
                println!("Pedido de parada enviado ao coletor.");
            } else {
                println!("Nenhum coletor em execução.");
            }
        }
        Modo::Terminal { amostras } => rodar_no_terminal(amostras, intervalo_ms),
        Modo::Encerrar { pid } => {
            if let Err(motivo) = checar_protecao(pid) {
                println!("{motivo}");
                std::process::exit(1);
            }
            let (encerrados, _) = controle::encerrar(&[pid]);
            if encerrados == 1 {
                println!("Processo {pid} encerrado.");
            } else {
                println!("O Windows recusou encerrar o processo {pid}.");
                std::process::exit(1);
            }
        }
        Modo::Prioridade { pid, prioridade } => {
            if let Err(motivo) = checar_protecao(pid) {
                println!("{motivo}");
                std::process::exit(1);
            }
            let (aplicados, _) = controle::definir_prioridade(&[pid], prioridade);
            if aplicados == 1 {
                println!("Prioridade do processo {pid}: {}.", prioridade.nome());
            } else {
                println!("O Windows recusou a mudança de prioridade do processo {pid}.");
                std::process::exit(1);
            }
        }
    }
}

fn rodar_no_terminal(amostras: u64, intervalo_ms: u64) {
    let mut coletor = Coletor::novo();
    let mut n: u64 = 0;
    while n < amostras {
        thread::sleep(Duration::from_millis(intervalo_ms));
        n += 1;
        imprimir(n, &coletor.amostrar(), &coletor.nucleos);
    }
}

fn imprimir(n: u64, a: &Amostra, nucleos: &[Vec<usize>]) {
    println!();
    println!(
        "Prumo · amostra {n} · média de {} s · custo do Prumo: {}% CPU · {} MB · coleta {} ms",
        num(a.decorrido, 1),
        num(a.custo_cpu, 2),
        num(a.custo_mem_mb, 1),
        num(a.ms_coleta, 1)
    );
    println!(
        "CPU · tempo   {}% de {} lógicos · turbo {} · processos {}% + vida curta e não atribuído {}%",
        num(a.cpu_total, 1),
        a.por_logico.len(),
        a.turbo.map(|t| format!("{}%", num(t, 0))).unwrap_or_else(|| "?".into()),
        num(a.cpu_processos, 1),
        num(a.cpu_nao_atribuida(), 1)
    );
    let por_nucleo: Vec<String> = nucleos
        .iter()
        .map(|logicos| {
            let valores: Vec<String> = logicos
                .iter()
                .map(|l| format!("{:>3}", a.por_logico.get(*l).copied().unwrap_or(0.0).round() as i64))
                .collect();
            format!("[{}]", valores.join(" "))
        })
        .collect();
    println!("  por núcleo  {}", por_nucleo.join(" "));
    println!(
        "Memória       {} GB = programas {} + Windows {} + compartilhado e outros {} (por diferença) + cache liberável {} + modificada {} + livre {}",
        gb(a.total_ram),
        gb(a.programas),
        gb(a.windows),
        gb(a.compartilhado_e_outros),
        gb(a.cache),
        gb(a.modificada),
        gb(a.livre)
    );
    println!("  em uso      {} GB · {} processos", gb(a.em_uso), a.n_processos);
    match (&a.gpu, a.vram_dedicada) {
        (Some((tipo, uso)), Some(dedicada)) => println!(
            "GPU           {} {}% (motor mais ocupado) · VRAM dedicada {} GB · soma dos processos {} GB",
            tipo,
            num(*uso, 1),
            gb(dedicada),
            gb(a.vram_processos)
        ),
        _ => println!("GPU           contadores indisponíveis"),
    }
    println!(
        "  {:<28} {:>6} {:>10} {:>14} {:>12} {:>9}",
        "processo", "CPU", "privada", "com compart.", "GPU", "VRAM"
    );
    for g in a.grupos.iter().take(6) {
        let rotulo =
            if g.processos() > 1 { format!("{} ({})", g.nome, g.processos()) } else { g.nome.clone() };
        let gpu = g.gpu.as_ref().map(|(tipo, uso)| format!("{tipo} {}%", num(*uso, 1))).unwrap_or_else(|| "—".into());
        let vram = if g.vram > 0.0 { format!("{} GB", gb(g.vram)) } else { "—".into() };
        println!(
            "  {:<28} {:>5}% {:>7} GB {:>11} GB {:>12} {:>9}",
            rotulo,
            num(g.cpu, 1),
            gb(g.privada),
            gb(g.working_set),
            gpu,
            vram
        );
    }
}
