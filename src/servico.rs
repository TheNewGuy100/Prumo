//! O coletor como processo próprio: mede em segundo plano, guarda o histórico
//! recente e entrega as amostras a quem conectar no pipe.
//!
//! Só escreve dados; não aceita nenhum pedido de quem conecta.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::fs::File;
use std::io::Write;
use std::os::windows::io::FromRawHandle;
use std::ptr;
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::coletor::Coletor;
use crate::protocolo::{
    montar_quadro, Apresentacao, NOME_DO_EVENTO_DE_PARADA, NOME_DO_PIPE, QUADRO_AMOSTRA, QUADRO_APRESENTACAO,
};

type Handle = isize;

#[link(name = "kernel32")]
extern "system" {
    fn CreateNamedPipeW(
        nome: *const u16,
        modo_de_abertura: u32,
        modo_do_pipe: u32,
        instancias: u32,
        buffer_de_saida: u32,
        buffer_de_entrada: u32,
        espera_padrao: u32,
        seguranca: *const c_void,
    ) -> Handle;
    fn ConnectNamedPipe(pipe: Handle, sobreposto: *mut c_void) -> i32;
    fn GetLastError() -> u32;
    fn CloseHandle(handle: Handle) -> i32;
    fn CreateEventW(seguranca: *const c_void, manual: i32, inicial: i32, nome: *const u16) -> Handle;
    fn OpenEventW(acesso: u32, herdar: i32, nome: *const u16) -> Handle;
    fn SetEvent(evento: Handle) -> i32;
    fn WaitForSingleObject(handle: Handle, ms: u32) -> u32;
    fn GetCurrentProcess() -> Handle;
    fn LocalFree(memoria: *mut c_void) -> *mut c_void;
}

#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(processo: Handle, acesso: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(token: Handle, classe: u32, info: *mut c_void, tamanho: u32, devolvido: *mut u32) -> i32;
    fn ConvertSidToStringSidW(sid: *mut c_void, texto: *mut *mut u16) -> i32;
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        sddl: *const u16,
        revisao: u32,
        descritor: *mut *mut c_void,
        tamanho: *mut u32,
    ) -> i32;
}

#[repr(C)]
struct SecurityAttributes {
    tamanho: u32,
    descritor: *mut c_void,
    herdar: i32,
}

const TOKEN_QUERY: u32 = 0x0008;
const TOKEN_USER: u32 = 1;

/// Descritor de segurança que dá acesso ao pipe só ao usuário atual e ao
/// sistema. As amostras levam linhas de comando, que podem ter dados sensíveis.
/// Devolve nulo se não conseguir montar; nesse caso vale a permissão padrão.
fn descritor_do_usuario() -> *mut c_void {
    unsafe {
        let mut token: Handle = 0;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return ptr::null_mut();
        }
        // TOKEN_USER começa com o ponteiro para o SID, que fica no mesmo buffer.
        let mut buffer: Vec<u64> = vec![0; 64];
        let mut devolvido: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TOKEN_USER,
            buffer.as_mut_ptr() as *mut c_void,
            (buffer.len() * 8) as u32,
            &mut devolvido,
        );
        CloseHandle(token);
        if ok == 0 {
            return ptr::null_mut();
        }
        let sid = buffer[0] as *mut c_void;
        let mut texto: *mut u16 = ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut texto) == 0 || texto.is_null() {
            return ptr::null_mut();
        }
        let mut n = 0usize;
        while *texto.add(n) != 0 {
            n += 1;
        }
        let sid_em_texto = String::from_utf16_lossy(std::slice::from_raw_parts(texto, n));
        LocalFree(texto as *mut c_void);

        // D:P = lista de acesso protegida; GA = acesso total; SY = sistema.
        let sddl = largo(&format!("D:P(A;;GA;;;{sid_em_texto})(A;;GA;;;SY)"));
        let mut descritor: *mut c_void = ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), 1, &mut descritor, ptr::null_mut()) == 0 {
            return ptr::null_mut();
        }
        descritor // vive enquanto o coletor rodar
    }
}

const PIPE_ACCESS_OUTBOUND: u32 = 0x0000_0002;
const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x0000_0008;
const PIPE_UNLIMITED_INSTANCES: u32 = 255;
const INVALID_HANDLE: Handle = -1;
const ERROR_PIPE_CONNECTED: u32 = 535;
const EVENT_MODIFY_STATE: u32 = 0x0002;
const INFINITE: u32 = 0xFFFF_FFFF;

/// Amostras de CPU guardadas para quem conectar depois.
pub const HISTORICO: usize = 60;
/// Sem ninguém conectado por este tempo, o coletor iniciado pela janela sai.
const SEGUNDOS_SEM_CLIENTES: f64 = 15.0;

fn largo(texto: &str) -> Vec<u16> {
    texto.encode_utf16().chain(std::iter::once(0)).collect()
}

fn criar_pipe(primeiro: bool, descritor: *mut c_void) -> Handle {
    let nome = largo(NOME_DO_PIPE);
    let mut modo = PIPE_ACCESS_OUTBOUND;
    if primeiro {
        modo |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    let seguranca =
        SecurityAttributes { tamanho: std::mem::size_of::<SecurityAttributes>() as u32, descritor, herdar: 0 };
    let ponteiro: *const c_void =
        if descritor.is_null() { ptr::null() } else { &seguranca as *const SecurityAttributes as *const c_void };
    unsafe {
        CreateNamedPipeW(
            nome.as_ptr(),
            modo,
            PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            1024 * 1024,
            0,
            0,
            ponteiro,
        )
    }
}

#[derive(Default)]
struct Compartilhado {
    clientes: Vec<SyncSender<Arc<Vec<u8>>>>,
    historico_cpu: VecDeque<f32>,
}

/// Roda até receber o pedido de parada. Com `sair_sem_clientes`, também sai
/// quando ninguém está conectado há algum tempo.
pub fn rodar(intervalo_ms: u64, sair_sem_clientes: bool) {
    let descritor = descritor_do_usuario();
    if descritor.is_null() {
        eprintln!("Aviso: não foi possível restringir o pipe ao usuário atual; vale a permissão padrão do Windows.");
    }
    let primeiro = criar_pipe(true, descritor);
    if primeiro == INVALID_HANDLE {
        eprintln!("O coletor do Prumo já está em execução.");
        return;
    }

    let nome_evento = largo(NOME_DO_EVENTO_DE_PARADA);
    let evento = unsafe { CreateEventW(ptr::null(), 1, 0, nome_evento.as_ptr()) };
    if evento != 0 {
        thread::spawn(move || {
            unsafe { WaitForSingleObject(evento, INFINITE) };
            std::process::exit(0);
        });
    }

    let mut coletor = Coletor::novo();
    let nucleos = coletor.nucleos.clone();
    let compartilhado = Arc::new(Mutex::new(Compartilhado::default()));

    let para_amostrar = compartilhado.clone();
    thread::spawn(move || {
        let mut ocioso = 0.0;
        loop {
            thread::sleep(Duration::from_millis(intervalo_ms));
            let amostra = coletor.amostrar();
            let quadro = Arc::new(montar_quadro(QUADRO_AMOSTRA, &amostra.codificar()));

            let mut c = para_amostrar.lock().unwrap();
            if c.historico_cpu.len() == HISTORICO {
                c.historico_cpu.pop_front();
            }
            c.historico_cpu.push_back(amostra.cpu_total as f32);
            // Cliente lento perde a amostra; cliente que saiu é removido.
            c.clientes
                .retain(|cliente| !matches!(cliente.try_send(quadro.clone()), Err(TrySendError::Disconnected(_))));

            if sair_sem_clientes {
                if c.clientes.is_empty() {
                    ocioso += amostra.decorrido;
                    if ocioso >= SEGUNDOS_SEM_CLIENTES {
                        std::process::exit(0);
                    }
                } else {
                    ocioso = 0.0;
                }
            }
        }
    });

    let mut pipe = primeiro;
    loop {
        let conectou = unsafe { ConnectNamedPipe(pipe, ptr::null_mut()) } != 0
            || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if !conectou {
            unsafe { CloseHandle(pipe) };
            pipe = criar_pipe(false, descritor);
            if pipe == INVALID_HANDLE {
                return;
            }
            continue;
        }

        // O File passa a ser dono do handle e o fecha ao sair de escopo.
        let mut arquivo = unsafe { File::from_raw_handle(pipe as *mut c_void) };
        {
            let mut c = compartilhado.lock().unwrap();
            let apresentacao = Apresentacao {
                nucleos: nucleos.clone(),
                historico_cpu: c.historico_cpu.iter().copied().collect(),
            };
            let quadro = montar_quadro(QUADRO_APRESENTACAO, &apresentacao.codificar());
            if arquivo.write_all(&quadro).is_ok() {
                let (envio, recebimento) = sync_channel::<Arc<Vec<u8>>>(4);
                c.clientes.push(envio);
                thread::spawn(move || {
                    for quadro in recebimento {
                        if arquivo.write_all(&quadro).is_err() {
                            break;
                        }
                    }
                });
            }
        }

        pipe = criar_pipe(false, descritor);
        if pipe == INVALID_HANDLE {
            return;
        }
    }
}

/// Pede ao coletor em execução que encerre. Devolve se havia um coletor.
pub fn parar() -> bool {
    let nome = largo(NOME_DO_EVENTO_DE_PARADA);
    let evento = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, nome.as_ptr()) };
    if evento == 0 {
        return false;
    }
    unsafe {
        SetEvent(evento);
        CloseHandle(evento);
    }
    true
}
