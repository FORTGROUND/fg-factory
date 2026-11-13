//! FG Factory Setup: configura os runners do FG Factory nesta máquina.
//!
//! O app é a interface; quem faz o trabalho é o `runner/setup.sh` do repositório fg-factory, que
//! o app baixa (ou atualiza) e chama. As respostas, inclusive os tokens, vão para o script pela
//! entrada padrão: nunca em argumentos, arquivos temporários ou no log.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod host;
mod model;

use eframe::egui::{self, Color32, RichText};
use host::Host;
use model::{Agent, Answers, Check, CodexLogin, LineKind, LogLine};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

const ICON_PNG: &[u8] = include_bytes!("../assets/icon-256.png");
const TOKENS_URL: &str = "https://tracker.fortground.com.br/account/?tab=tokens";
const DEVICE_URL: &str = "https://github.com/login/device";
const README_URL: &str = "https://github.com/FORTGROUND/fg-factory#runners-próprios";

const ACCENT: Color32 = Color32::from_rgb(0x22, 0xE5, 0xC2);
const BG: Color32 = Color32::from_rgb(0x12, 0x15, 0x1C);
const PANEL: Color32 = Color32::from_rgb(0x1A, 0x1E, 0x27);
const BORDER: Color32 = Color32::from_rgb(0x26, 0x2B, 0x36);
const MUTED: Color32 = Color32::from_rgb(0x9A, 0xA3, 0xB2);
const DANGER: Color32 = Color32::from_rgb(0xF0, 0x6B, 0x6B);
const WARN: Color32 = Color32::from_rgb(0xF2, 0xC1, 0x4E);

/// Acha (ou baixa) o fg-factory e imprime `::dir::<pasta do runner>`. Prefere a pasta dos
/// runners que já rodam nesta máquina, para não criar uma segunda configuração.
const BOOTSTRAP: &str = r#"
set -e
command -v git >/dev/null || { echo "::fail::Falta o git. No Ubuntu/WSL: sudo apt install git. No macOS: xcode-select --install."; exit 1; }
dir=""
running="$(docker ps --filter label=com.docker.compose.service=runner1 --format '{{.Label "com.docker.compose.project.working_dir"}}' 2>/dev/null | head -1 || true)"
if [ -n "$running" ] && [ -d "$running" ]; then
  if [ -z "$(git -C "$running/.." status --porcelain 2>/dev/null)" ]; then git -C "$running/.." pull -q --ff-only 2>/dev/null || true; fi
  [ -f "$running/setup.sh" ] && dir="$running"
fi
if [ -z "$dir" ]; then
  base="$HOME/.fg-factory/fg-factory"
  if [ -d "$base/.git" ]; then
    git -C "$base" pull -q --ff-only 2>/dev/null || true
  else
    mkdir -p "$HOME/.fg-factory"
    git clone -q https://github.com/FORTGROUND/fg-factory.git "$base"
  fi
  dir="$base/runner"
fi
[ -f "$dir/setup.sh" ] || { echo "::fail::Não achei o setup.sh em $dir."; exit 1; }
echo "::dir::$dir"
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
    Bootstrap,
    Check,
    InstallTools,
    GhLogin,
    Apply,
}

enum Msg {
    Line(String),
    Dir(String),
    Checked(Result<Box<Check>, String>),
    Finished(Task, bool),
}

struct App {
    host: Host,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    ctx: egui::Context,
    icon: egui::TextureHandle,
    dir: Option<String>,
    check: Option<Check>,
    check_error: Option<String>,
    busy: Option<Task>,
    log: Vec<LogLine>,
    answers: Answers,
    form_ready: bool,
    finished: Option<bool>,
    show_log: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        style(&cc.egui_ctx);
        let (tx, rx) = channel();
        let icon = cc.egui_ctx.load_texture("icon", icon_image(), egui::TextureOptions::LINEAR);
        let mut app = App {
            host: Host::detect(),
            tx,
            rx,
            ctx: cc.egui_ctx.clone(),
            icon,
            dir: None,
            check: None,
            check_error: None,
            busy: None,
            log: Vec::new(),
            answers: Answers::default(),
            form_ready: false,
            finished: None,
            show_log: false,
        };
        app.run(Task::Bootstrap);
        app
    }

    fn setup_sh(&self) -> String {
        format!("bash {}", shell_quote(&format!("{}/setup.sh", self.dir.as_deref().unwrap_or("."))))
    }

    /// Começa uma tarefa em segundo plano; as linhas chegam pela fila.
    fn run(&mut self, task: Task) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(task);
        let host = self.host;
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let wake = move || ctx.request_repaint();
        let line = host::sender(&tx, Msg::Line, wake.clone());
        let setup = self.setup_sh();
        let input = (task == Task::Apply).then(|| self.answers.to_input());
        if task == Task::Apply {
            self.finished = None;
            self.show_log = true;
            self.log.clear();
        }
        thread::spawn(move || {
            let ok = match task {
                Task::Bootstrap => {
                    let tx2 = tx.clone();
                    let wake2 = wake.clone();
                    let mut found = false;
                    let ok = host.stream(BOOTSTRAP, None, {
                        let line = line.clone();
                        move |text: String| {
                            if let Some(dir) = text.strip_prefix("::dir::") {
                                let _ = tx2.send(Msg::Dir(dir.to_owned()));
                                wake2();
                            } else {
                                line(text);
                            }
                        }
                    });
                    found |= ok;
                    found
                }
                Task::Check => {
                    let result = host
                        .output(&format!("{setup} --check"))
                        .and_then(|text| model::parse_check(&text));
                    let ok = result.is_ok();
                    let _ = tx.send(Msg::Checked(result.map(Box::new)));
                    ok
                }
                Task::InstallTools => host.stream(&format!("{setup} --install-tools"), None, line),
                Task::GhLogin => host.stream(&format!("{setup} --gh-login"), None, line),
                Task::Apply => host.stream(&format!("{setup} --apply"), input, line),
            };
            let _ = tx.send(Msg::Finished(task, ok));
            wake();
        });
    }

    fn drain(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Line(text) => {
                    if let Some(line) = LogLine::parse(&text) {
                        self.log.push(line);
                    }
                }
                Msg::Dir(dir) => self.dir = Some(dir),
                Msg::Checked(Ok(check)) => {
                    if !self.form_ready && check.ready_for_form() {
                        self.answers = Answers::from_check(&check);
                        self.form_ready = true;
                    }
                    self.check = Some(*check);
                    self.check_error = None;
                }
                Msg::Checked(Err(e)) => self.check_error = Some(e),
                Msg::Finished(task, ok) => {
                    self.busy = None;
                    match task {
                        Task::Bootstrap if ok && self.dir.is_some() => self.run(Task::Check),
                        Task::Bootstrap => self.check_error = Some("Não consegui preparar o fg-factory nesta máquina.".into()),
                        Task::InstallTools | Task::GhLogin => self.run(Task::Check),
                        Task::Apply => {
                            self.finished = Some(ok);
                            self.answers.forget_secrets();
                            self.run(Task::Check);
                        }
                        Task::Check => {}
                    }
                }
            }
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("header")
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(20, 14)))
            .show(ui, |ui| self.header(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(20, 6)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.body(ui));
            });
    }
}

impl App {
    fn header(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(egui::Image::new((self.icon.id(), egui::vec2(40.0, 40.0))));
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("FG Factory").size(20.0).strong().color(Color32::WHITE));
                ui.label(RichText::new("Runners desta máquina: configure uma vez, valem para todos os repositórios.").color(MUTED));
            });
        });
    }

    fn body(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        let Some(check) = self.check.clone() else {
            if let Some(error) = self.check_error.clone() {
                card(ui, "Algo deu errado", |ui| {
                    ui.label(RichText::new(error).color(DANGER));
                    if ui.button("Tentar de novo").clicked() {
                        self.check_error = None;
                        self.run(if self.dir.is_some() { Task::Check } else { Task::Bootstrap });
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Conferindo {} …", self.host.label()));
                });
            }
            self.log_view(ui);
            return;
        };

        self.machine_card(ui, &check);
        if !check.ready_for_form() {
            self.log_view(ui);
            return;
        }
        if !self.form_ready {
            self.answers = Answers::from_check(&check);
            self.form_ready = true;
        }
        let locked = self.busy.is_some();
        ui.add_enabled_ui(!locked, |ui| {
            self.runners_card(ui, &check);
            self.agents_card(ui);
            self.credentials_card(ui, &check);
        });
        self.apply_bar(ui, &check);
        self.device_code(ui);
        self.log_view(ui);
    }

    fn machine_card(&mut self, ui: &mut egui::Ui, check: &Check) {
        card(ui, "Esta máquina", |ui| {
            if !check.missing_tools.is_empty() {
                status(ui, false, &format!("Faltam ferramentas: {}", check.missing_tools.join(", ")));
                if ui.add_enabled(self.busy.is_none(), egui::Button::new("Instalar (sem senha de administrador)")).clicked() {
                    self.run(Task::InstallTools);
                }
                return;
            }
            let d = &check.docker;
            if !d.installed {
                status(ui, false, "Docker não instalado. Instale o Docker Desktop (no Windows, com a integração do WSL ligada).");
            } else if !d.running {
                status(ui, false, "O Docker não está rodando. Abra o Docker Desktop.");
            } else {
                status(ui, true, &format!("Docker rodando · {} núcleos · {} GB para os contêineres", d.cpus, d.memory_gb));
            }
            if !check.git {
                status(ui, false, "Falta o git.");
            }
            let gh = &check.gh;
            if !gh.installed {
                status(ui, false, "Falta o GitHub CLI (gh).");
                if ui.add_enabled(self.busy.is_none(), egui::Button::new("Instalar o gh")).clicked() {
                    self.run(Task::InstallTools);
                }
            } else if !gh.logged_in {
                status(ui, false, "O GitHub CLI não está logado.");
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.busy.is_none(), egui::Button::new("Entrar no GitHub")).clicked() {
                        self.log.clear();
                        self.run(Task::GhLogin);
                    }
                    ui.label(RichText::new("Mostra um código para digitar no navegador.").color(MUTED));
                });
            } else {
                status(ui, true, &format!("GitHub: {}", gh.user));
            }
            if check.current.runners_running > 0 {
                status(ui, true, &format!("{} runner(s) rodando agora nesta máquina", check.current.runners_running));
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(self.busy.is_none(), egui::Button::new("Conferir de novo")).clicked() {
                    self.run(Task::Check);
                }
                if self.busy == Some(Task::Check) {
                    ui.spinner();
                }
            });
        });
    }

    fn runners_card(&mut self, ui: &mut egui::Ui, check: &Check) {
        card(ui, "Runners", |ui| {
            let a = &mut self.answers;
            egui::Grid::new("runners").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Organização do GitHub");
                let mut orgs = check.gh.admin_orgs.clone();
                if !orgs.contains(&a.org) {
                    orgs.insert(0, a.org.clone());
                }
                egui::ComboBox::from_id_salt("org").selected_text(&a.org).show_ui(ui, |ui| {
                    for org in orgs {
                        ui.selectable_value(&mut a.org, org.clone(), org);
                    }
                });
                ui.end_row();
                ui.label("Nome desta máquina");
                ui.add(egui::TextEdit::singleline(&mut a.machine).desired_width(240.0));
                ui.end_row();
            });
            if !check.gh.admin_orgs.is_empty() && !check.gh.admin_orgs.contains(&a.org) {
                ui.label(RichText::new(format!("Sua conta do GitHub não é owner de {}: o registro dos runners vai falhar.", a.org)).color(WARN));
            }
            ui.add_space(6.0);
            ui.label("Quantas tarefas ao mesmo tempo? Cada uma é um runner.");
            let s = &check.suggestion;
            ui.horizontal(|ui| {
                for n in 1..=s.max.max(1) {
                    let text = if n == s.runners { format!("{n} (sugerido)") } else { n.to_string() };
                    ui.selectable_value(&mut a.runners, n, text);
                }
            });
            ui.label(
                RichText::new(format!(
                    "Sugestão pelo que o Docker tem: {} núcleos e {} GB. Num build pesado cada runner chega a {} núcleos e {} GB, \
                     e cada tarefa simultânea é mais uma sessão do agente na sua conta.",
                    check.docker.cpus, check.docker.memory_gb, s.per_runner_cpus, s.per_runner_gb
                ))
                .color(MUTED),
            );
        });
    }

    fn agents_card(&mut self, ui: &mut egui::Ui) {
        card(ui, "Agentes", |ui| {
            let a = &mut self.answers;
            egui::Grid::new("agents").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Quem corrige");
                ui.radio_value(&mut a.agent, Agent::Claude, "Claude Code");
                ui.radio_value(&mut a.agent, Agent::Codex, "Codex");
                ui.end_row();
                ui.label("Quem revisa");
                ui.radio_value(&mut a.review_agent, Agent::Claude, "Claude Code");
                ui.radio_value(&mut a.review_agent, Agent::Codex, "Codex");
                ui.end_row();
            });
            ui.label(RichText::new("A revisão é uma segunda sessão, sem o contexto de quem corrigiu, que pode barrar o PR. Pode ser outro agente.").color(MUTED));
            ui.add_space(4.0);
            ui.checkbox(&mut a.docker_in_runner, "Os testes podem subir contêineres (banco etc.): um Docker próprio por runner");
        });
    }

    fn credentials_card(&mut self, ui: &mut egui::Ui, check: &Check) {
        let host = self.host;
        card(ui, "Credenciais", |ui| {
            ui.label(RichText::new("Digitadas uma vez, valem para todos os runners desta máquina. Ficam só no .env dos runners, que só o seu usuário lê.").color(MUTED));
            ui.add_space(4.0);
            let a = &mut self.answers;
            if a.uses(Agent::Claude) {
                ui.label(RichText::new("Claude Code").strong());
                secret_field(ui, &mut a.claude, check.saved.claude, "Token de claude setup-token, ou chave da API da Anthropic");
                ui.label(RichText::new("Gere no terminal com claude setup-token: ele abre o navegador e imprime um token sk-ant-oat…").color(MUTED));
                ui.add_space(6.0);
            }
            if a.uses(Agent::Codex) {
                ui.label(RichText::new("Codex").strong());
                ui.horizontal(|ui| {
                    ui.radio_value(&mut a.codex_login, CodexLogin::ChatGpt, "Assinatura do ChatGPT");
                    ui.radio_value(&mut a.codex_login, CodexLogin::Api, "Chave da API da OpenAI");
                });
                if a.codex_login == CodexLogin::Api {
                    secret_field(ui, &mut a.openai, check.saved.openai, "sk-…");
                } else if check.saved.codex_login {
                    status(ui, true, "Codex já logado nesta máquina");
                } else {
                    ui.label(RichText::new("Ao configurar, aparece um código para você digitar no navegador. Um login serve a todos os runners.").color(MUTED));
                }
                ui.add_space(6.0);
            }
            ui.label(RichText::new("FG Tracker").strong());
            secret_field(ui, &mut a.tracker, check.saved.tracker, "Token do modelo FG Factory");
            ui.horizontal(|ui| {
                ui.label(RichText::new("No painel: Conta, Tokens de acesso, modelo FG Factory, restrito à organização.").color(MUTED));
                if ui.link("Abrir").clicked() {
                    host.open_url(TOKENS_URL);
                }
            });
            ui.add_space(6.0);
            ui.checkbox(&mut a.delete_old_secrets, "Apagar segredos antigos dos repositórios, que passariam por cima destas credenciais");
        });
    }

    fn apply_bar(&mut self, ui: &mut egui::Ui, check: &Check) {
        ui.add_space(8.0);
        let problems = self.answers.problems(check);
        ui.horizontal(|ui| {
            let button = egui::Button::new(RichText::new("Configurar esta máquina").strong().color(BG)).fill(ACCENT).min_size(egui::vec2(220.0, 34.0));
            let enabled = problems.is_empty() && self.busy.is_none();
            if ui.add_enabled(enabled, button).clicked() {
                self.run(Task::Apply);
            }
            if self.busy == Some(Task::Apply) {
                ui.spinner();
                ui.label("Configurando… pode levar alguns minutos na primeira vez (a imagem dos runners é montada).");
            }
        });
        for problem in &problems {
            ui.label(RichText::new(format!("• {problem}")).color(MUTED));
        }
        match self.finished {
            Some(true) => {
                ui.add_space(6.0);
                let done = self.log.iter().rev().find(|l| l.kind == LineKind::Done).map(|l| l.text.clone());
                card(ui, "Pronto", |ui| {
                    ui.label(RichText::new(done.unwrap_or_else(|| "Configurado.".into())).color(ACCENT));
                    ui.label(RichText::new("Para mudar algo depois (mais runners, outro agente, um token novo), abra este app de novo.").color(MUTED));
                });
            }
            Some(false) => {
                let fail = self.log.iter().rev().find(|l| l.kind == LineKind::Fail).map(|l| l.text.clone());
                ui.label(RichText::new(fail.unwrap_or_else(|| "A configuração parou. Veja os detalhes abaixo.".into())).color(DANGER));
            }
            None => {}
        }
    }

    /// O código do login do GitHub ou do Codex, grande, com o botão para abrir a página.
    fn device_code(&mut self, ui: &mut egui::Ui) {
        let Some(code) = self.log.iter().rev().find(|l| l.kind == LineKind::Code).map(|l| l.text.clone()) else { return };
        if !matches!(self.busy, Some(Task::GhLogin | Task::Apply)) {
            return;
        }
        let url = self
            .log
            .iter()
            .rev()
            .find_map(|l| model::find_url(&l.text))
            .unwrap_or_else(|| DEVICE_URL.to_owned());
        let host = self.host;
        card(ui, "Digite este código no navegador", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&code).size(28.0).monospace().strong().color(ACCENT));
                if ui.button("Copiar").clicked() {
                    ui.ctx().copy_text(code.clone());
                }
            });
            if ui.button(format!("Abrir {url}")).clicked() {
                host.open_url(&url);
            }
        });
    }

    fn log_view(&mut self, ui: &mut egui::Ui) {
        if self.log.is_empty() {
            return;
        }
        ui.add_space(8.0);
        egui::CollapsingHeader::new("Detalhes")
            .id_salt("log")
            .open(if self.busy == Some(Task::Apply) { Some(true) } else { None })
            .default_open(self.show_log)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(220.0).stick_to_bottom(true).show(ui, |ui| {
                    for line in &self.log {
                        let color = match line.kind {
                            LineKind::Step => Color32::WHITE,
                            LineKind::Ok | LineKind::Done => ACCENT,
                            LineKind::Warn => WARN,
                            LineKind::Fail => DANGER,
                            LineKind::Code => ACCENT,
                            LineKind::Plain => MUTED,
                        };
                        let text = match line.kind {
                            LineKind::Step => format!("▸ {}", line.text),
                            LineKind::Code => format!("Código: {}", line.text),
                            _ => line.text.clone(),
                        };
                        ui.label(RichText::new(text).monospace().size(12.0).color(color));
                    }
                });
            });
        let host = self.host;
        if ui.link("Como funciona (README do fg-factory)").clicked() {
            host.open_url(README_URL);
        }
    }
}

fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(15.0).strong().color(Color32::WHITE));
            ui.add_space(6.0);
            body(ui);
        });
    ui.add_space(10.0);
}

/// Uma linha de situação: bolinha verde (ok) ou vermelha (falta algo) e o texto.
fn status(ui: &mut egui::Ui, ok: bool, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.5, if ok { ACCENT } else { DANGER });
        ui.label(text);
    });
}

fn secret_field(ui: &mut egui::Ui, value: &mut String, saved: bool, hint: &str) {
    let hint = if saved { "Já gravado. Deixe vazio para manter".to_owned() } else { hint.to_owned() };
    ui.add(egui::TextEdit::singleline(value).password(true).hint_text(hint).desired_width(f32::INFINITY));
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.all_styles_mut(|style| {
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = PANEL;
        style.visuals.extreme_bg_color = Color32::from_rgb(0x0D, 0x10, 0x15);
        style.visuals.selection.bg_fill = ACCENT.linear_multiply(0.35);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);
        style.visuals.hyperlink_color = ACCENT;
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT);
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        for (text_style, font) in style.text_styles.iter_mut() {
            if *text_style != egui::TextStyle::Heading {
                font.size = font.size.max(13.5);
            }
        }
    });
}

fn icon_image() -> egui::ColorImage {
    let image = image::load_from_memory(ICON_PNG).expect("ícone").to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw())
}

fn window_icon() -> egui::IconData {
    let image = image::load_from_memory(ICON_PNG).expect("ícone").to_rgba8();
    egui::IconData { width: image.width(), height: image.height(), rgba: image.into_raw() }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("FG Factory Setup")
            .with_app_id("fg-factory-setup")
            .with_inner_size([700.0, std::env::var("FG_FACTORY_SETUP_TEST_HEIGHT").ok().and_then(|h| h.parse().ok()).unwrap_or(820.0)])
            .with_min_inner_size([560.0, 520.0])
            .with_icon(window_icon()),
        ..Default::default()
    };
    eframe::run_native("FG Factory Setup", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
