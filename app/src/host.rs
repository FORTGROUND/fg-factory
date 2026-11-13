//! Onde os comandos rodam. No macOS e no Linux, na própria máquina; no Windows, dentro do WSL,
//! onde estão o Docker (pela integração do Docker Desktop), o `gh` e os runners.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Local,
    Wsl,
}

impl Host {
    pub fn detect() -> Self {
        if cfg!(windows) { Host::Wsl } else { Host::Local }
    }

    pub fn label(self) -> &'static str {
        match self {
            Host::Local => "esta máquina",
            Host::Wsl => "o WSL (Ubuntu) desta máquina",
        }
    }

    /// Um script de bash num shell de login, para valer o PATH da pessoa (gh em ~/.local/bin,
    /// Docker Desktop). O script vai como argumento; dados secretos nunca: esses vão pela entrada.
    fn command(self, script: &str) -> Command {
        let mut command = match self {
            Host::Local => {
                // Aberto pelo Finder, o app não herda o PATH do terminal: o Homebrew e o Docker
                // Desktop ficam em /opt/homebrew/bin e /usr/local/bin.
                let script = format!("export PATH=\"$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$PATH\"; {script}");
                let mut c = Command::new("bash");
                c.args(["-lc", &script]);
                c
            }
            Host::Wsl => {
                let mut c = Command::new("wsl.exe");
                c.args(["-e", "bash", "-lc", script]);
                c
            }
        };
        hide_window(&mut command);
        command
    }

    /// Roda e devolve a saída inteira (para respostas curtas, como o `--check`).
    pub fn output(self, script: &str) -> Result<String, String> {
        let out = self
            .command(script)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| self.spawn_error(e))?;
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        if out.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr).replace('\r', "");
            Err(last_lines(&format!("{stdout}\n{stderr}"), 6))
        }
    }

    /// Roda mandando `input` pela entrada e entrega cada linha da saída (e dos erros) a `line`
    /// conforme chega. Devolve se terminou bem.
    pub fn stream(self, script: &str, input: Option<String>, line: impl Fn(String) + Send + Clone + 'static) -> bool {
        let mut child = match self
            .command(script)
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                line(format!("::fail::{}", self.spawn_error(e)));
                return false;
            }
        };
        if let (Some(mut stdin), Some(input)) = (child.stdin.take(), input) {
            // Fecha a entrada ao terminar: o script lê até o fim.
            let _ = stdin.write_all(input.as_bytes());
        }
        let mut readers = Vec::new();
        if let Some(out) = child.stdout.take() {
            let line = line.clone();
            readers.push(thread::spawn(move || pump(out, line)));
        }
        if let Some(err) = child.stderr.take() {
            let line = line.clone();
            readers.push(thread::spawn(move || pump(err, line)));
        }
        for reader in readers {
            let _ = reader.join();
        }
        child.wait().map(|status| status.success()).unwrap_or(false)
    }

    fn spawn_error(self, e: std::io::Error) -> String {
        match self {
            Host::Wsl => format!("Não consegui abrir o WSL ({e}). Instale o WSL com o Ubuntu e o Docker Desktop com a integração do WSL ligada."),
            Host::Local => format!("Não consegui rodar o bash ({e})."),
        }
    }

    /// Abre um endereço no navegador da pessoa.
    pub fn open_url(self, url: &str) {
        let mut command = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", "start", "", url]);
            c
        } else if cfg!(target_os = "macos") {
            let mut c = Command::new("open");
            c.arg(url);
            c
        } else if is_wsl() {
            let mut c = Command::new("explorer.exe");
            c.arg(url);
            c
        } else {
            let mut c = Command::new("xdg-open");
            c.arg(url);
            c
        };
        hide_window(&mut command);
        let _ = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }
}

fn pump(reader: impl std::io::Read, line: impl Fn(String)) {
    for text in BufReader::new(reader).lines().map_while(Result::ok) {
        line(strip_ansi(&text.replace('\r', "")));
    }
}

fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Tira as cores do terminal (o Codex e o Docker as usam).
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn is_wsl() -> bool {
    std::fs::read_to_string("/proc/version")
        .map(|v| v.to_lowercase().contains("microsoft"))
        .unwrap_or(false)
}

#[cfg(windows)]
fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_: &mut Command) {}

/// Manda as linhas de um comando para a interface.
pub fn sender<T: Send + 'static>(tx: &Sender<T>, wrap: fn(String) -> T, wake: impl Fn() + Send + Clone + 'static) -> impl Fn(String) + Send + Clone + 'static {
    let tx = tx.clone();
    move |text| {
        let _ = tx.send(wrap(text));
        wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_are_removed_and_text_kept() {
        assert_eq!(strip_ansi("\u{1b}[1;32mABCD-1234\u{1b}[0m pronto"), "ABCD-1234 pronto");
        assert_eq!(strip_ansi("sem cor"), "sem cor");
    }

    #[test]
    fn only_the_last_lines_of_an_error_are_kept() {
        assert_eq!(last_lines("a\n\nb\nc\nd", 2), "c\nd");
    }
}
