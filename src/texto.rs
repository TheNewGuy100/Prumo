//! Formatação de números para exibição, com vírgula decimal.

const GB: f64 = 1024.0 * 1024.0 * 1024.0;

pub fn num(valor: f64, casas: usize) -> String {
    format!("{valor:.casas$}").replace('.', ",")
}

/// Bytes com a unidade que couber: GB com uma casa, ou MB abaixo de 1 GB.
pub fn tamanho(bytes: f64) -> String {
    if bytes >= 0.95 * GB {
        format!("{} GB", num(bytes / GB, 1))
    } else {
        format!("{} MB", num(bytes / (1024.0 * 1024.0), 0))
    }
}

/// Bytes por segundo: MB/s com uma casa, kB/s abaixo de 1 MB/s, traço se parado.
pub fn taxa(bytes_por_segundo: f64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if bytes_por_segundo < 512.0 {
        "—".into()
    } else if bytes_por_segundo >= MB {
        format!("{} MB/s", num(bytes_por_segundo / MB, 1))
    } else {
        format!("{} kB/s", num(bytes_por_segundo / 1024.0, 0))
    }
}

/// Bytes em GB com uma casa.
pub fn gb(bytes: f64) -> String {
    num(bytes / GB, 1)
}
