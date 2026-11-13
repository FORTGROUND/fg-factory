//! O que a máquina tem (`setup.sh --check`), as respostas da pessoa e as linhas do log.

use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Check {
    pub os: String,
    pub dir: String,
    pub git: bool,
    pub missing_tools: Vec<String>,
    pub docker: Docker,
    pub gh: Gh,
    pub current: Current,
    pub saved: Saved,
    pub suggestion: Suggestion,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Docker {
    pub installed: bool,
    pub running: bool,
    pub cpus: u32,
    pub memory_gb: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Gh {
    pub installed: bool,
    pub logged_in: bool,
    pub user: String,
    pub admin_orgs: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Current {
    pub org: String,
    pub machine: String,
    pub agent: String,
    pub review_agent: String,
    pub docker_in_runner: bool,
    pub runners_running: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Saved {
    /// Alguma credencial do Claude: o token da conta ou a chave da API.
    pub claude: bool,
    /// A credencial do Claude gravada é uma chave da API.
    pub anthropic_key: bool,
    pub openai: bool,
    pub tracker: bool,
    pub codex_login: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Suggestion {
    pub runners: u32,
    pub max: u32,
    pub per_runner_cpus: u32,
    pub per_runner_gb: u32,
}

impl Default for Suggestion {
    fn default() -> Self {
        Suggestion { runners: 1, max: 4, per_runner_cpus: 3, per_runner_gb: 6 }
    }
}

impl Check {
    /// O formulário só faz sentido com as ferramentas, o Docker e o GitHub prontos.
    pub fn ready_for_form(&self) -> bool {
        self.missing_tools.is_empty() && self.docker.running && self.git && self.gh.installed && self.gh.logged_in
    }
}

pub fn parse_check(text: &str) -> Result<Check, String> {
    let start = text.find('{').ok_or_else(|| format!("O setup.sh não respondeu como esperado:\n{}", text.trim()))?;
    serde_json::from_str(&text[start..]).map_err(|e| format!("Resposta do setup.sh ilegível ({e})."))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    fn parse(text: &str) -> Self {
        if text == "codex" { Agent::Codex } else { Agent::Claude }
    }
    fn key(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexLogin {
    ChatGpt,
    Api,
}

/// As respostas do formulário. Os segredos ficam vazios quando a pessoa mantém o que está gravado.
#[derive(Clone, Debug)]
pub struct Answers {
    pub org: String,
    pub machine: String,
    pub runners: u32,
    pub agent: Agent,
    pub review_agent: Agent,
    pub docker_in_runner: bool,
    pub claude: String,
    pub codex_login: CodexLogin,
    pub openai: String,
    pub tracker: String,
    pub delete_old_secrets: bool,
}

impl Default for Answers {
    fn default() -> Self {
        Answers {
            org: "FORTGROUND".into(),
            machine: String::new(),
            runners: 1,
            agent: Agent::Claude,
            review_agent: Agent::Claude,
            docker_in_runner: true,
            claude: String::new(),
            codex_login: CodexLogin::ChatGpt,
            openai: String::new(),
            tracker: String::new(),
            delete_old_secrets: true,
        }
    }
}

impl Answers {
    pub fn from_check(check: &Check) -> Self {
        let c = &check.current;
        Answers {
            org: if c.org.is_empty() { "FORTGROUND".into() } else { c.org.clone() },
            machine: c.machine.clone(),
            runners: if c.runners_running > 0 { c.runners_running } else { check.suggestion.runners }.clamp(1, check.suggestion.max.max(1)),
            agent: Agent::parse(&c.agent),
            review_agent: Agent::parse(&c.review_agent),
            docker_in_runner: c.docker_in_runner,
            codex_login: if check.saved.openai { CodexLogin::Api } else { CodexLogin::ChatGpt },
            ..Answers::default()
        }
    }

    pub fn uses(&self, agent: Agent) -> bool {
        self.agent == agent || self.review_agent == agent
    }

    /// O que impede de configurar, em frases curtas. Vazio = pode configurar.
    pub fn problems(&self, check: &Check) -> Vec<String> {
        let mut out = Vec::new();
        if self.machine.trim().is_empty() {
            out.push("Dê um nome a esta máquina.".into());
        }
        if self.uses(Agent::Claude) && !check.saved.claude && self.claude.trim().is_empty() {
            out.push("Conecte a conta do Claude.".into());
        }
        if self.uses(Agent::Codex) {
            match self.codex_login {
                CodexLogin::Api if !check.saved.openai && self.openai.trim().is_empty() => out.push("Falta a chave da API da OpenAI.".into()),
                CodexLogin::ChatGpt if !check.saved.codex_login => out.push("Conecte a conta do ChatGPT.".into()),
                _ => {}
            }
        }
        if !check.saved.tracker && self.tracker.trim().is_empty() {
            out.push("Falta o token do FG Tracker.".into());
        }
        out
    }

    /// As respostas para `setup.sh --apply`, uma por linha. Só vão os segredos digitados agora.
    pub fn to_input(&self) -> String {
        let one_line = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        let mut lines = vec![
            format!("ORG={}", one_line(&self.org)),
            format!("MACHINE={}", self.machine.replace(['\n', '\r'], " ").trim()),
            format!("RUNNERS={}", self.runners),
            format!("AGENT={}", self.agent.key()),
            format!("REVIEW_AGENT={}", self.review_agent.key()),
            format!("DOCKER={}", if self.docker_in_runner { "s" } else { "n" }),
            format!("DELETE_OLD_SECRETS={}", if self.delete_old_secrets { "s" } else { "n" }),
        ];
        if self.uses(Agent::Codex) {
            lines.push(format!("CODEX_LOGIN={}", if self.codex_login == CodexLogin::Api { "api" } else { "chatgpt" }));
        }
        for (key, value) in [("CLAUDE_TOKEN", &self.claude), ("OPENAI_API_KEY", &self.openai), ("FG_TRACKER_TOKEN", &self.tracker)] {
            let value = one_line(value);
            if !value.is_empty() {
                lines.push(format!("{key}={value}"));
            }
        }
        lines.join("\n") + "\n"
    }

    /// Depois de configurar, os segredos saem da memória do app.
    pub fn forget_secrets(&mut self) {
        for secret in [&mut self.claude, &mut self.openai, &mut self.tracker] {
            secret.clear();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Step,
    Ok,
    Warn,
    Fail,
    Done,
    Code,
    Plain,
}

#[derive(Clone, Debug)]
pub struct LogLine {
    pub kind: LineKind,
    pub text: String,
}

impl LogLine {
    /// As marcas do setup.sh (`::step::`, `::ok::`...) viram o tipo da linha. Um código de login
    /// (o do GitHub, marcado, ou o do Codex, achado no texto) vira uma linha `Code`.
    pub fn parse(text: &str) -> Option<LogLine> {
        let text = text.trim_end();
        if text.trim().is_empty() {
            return None;
        }
        for (mark, kind) in [
            ("::step::", LineKind::Step),
            ("::ok::", LineKind::Ok),
            ("::warn::", LineKind::Warn),
            ("::fail::", LineKind::Fail),
            ("::done::", LineKind::Done),
            ("::code::", LineKind::Code),
        ] {
            if let Some(rest) = text.strip_prefix(mark) {
                let rest = rest.trim();
                let rest = if kind == LineKind::Code { find_code(rest).unwrap_or_else(|| rest.to_owned()) } else { rest.to_owned() };
                return Some(LogLine { kind, text: rest });
            }
        }
        if let Some(code) = find_code(text) {
            return Some(LogLine { kind: LineKind::Code, text: code });
        }
        Some(LogLine { kind: LineKind::Plain, text: text.to_owned() })
    }
}

/// Um código de login de dispositivo: `ABCD-1234` ou `ABCD-12345`, só letras maiúsculas e números.
pub fn find_code(text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .find(|word| {
            let mut parts = word.split('-');
            let (Some(a), Some(b), None) = (parts.next(), parts.next(), parts.next()) else { return false };
            let ok = |p: &str| p.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
            (4..=5).contains(&a.len()) && (4..=5).contains(&b.len()) && ok(a) && ok(b)
                && word.chars().any(|c| c.is_ascii_uppercase())
        })
        .map(str::to_owned)
}

const TOKEN_CHARS: fn(char) -> bool = |c| c.is_ascii_alphanumeric() || c == '-' || c == '_';
const URL_END: fn(char) -> bool = |c| c.is_whitespace() || "\"'<>)".contains(c);

/// O que o `claude setup-token` já mostrou: o endereço de login e, no fim, o token. Só vale o que
/// chegou inteiro (com algo depois, ou com o processo encerrado), para não pegar meio token.
pub fn claude_login(text: &str, ended: bool) -> (Option<String>, Option<String>) {
    let complete = |rest: &str, len: usize| ended || rest[len..].chars().next().is_some();
    let url = text.find("https://").and_then(|start| {
        text[start..].match_indices("https://").map(|(i, _)| start + i).find_map(|at| {
            let rest = &text[at..];
            let len = rest.find(URL_END).unwrap_or(rest.len());
            (rest[..len].contains("/oauth/authorize") && complete(rest, len)).then(|| rest[..len].to_owned())
        })
    });
    let token = text.find("sk-ant-").and_then(|start| {
        let rest = &text[start..];
        let len = rest.find(|c| !TOKEN_CHARS(c)).unwrap_or(rest.len());
        (len > 40 && complete(rest, len)).then(|| rest[..len].to_owned())
    });
    (url, token)
}

/// O primeiro endereço https de uma linha.
pub fn find_url(text: &str) -> Option<String> {
    let start = text.find("https://")?;
    let url: String = text[start..].chars().take_while(|c| !c.is_whitespace() && !"\"'<>)".contains(*c)).collect();
    Some(url.trim_end_matches(['.', ',']).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECK: &str = r#"aviso do shell
    {"os":"WSL","dir":"/x/runner","git":true,
     "docker":{"installed":true,"running":true,"cpus":24,"memoryGb":62},
     "gh":{"installed":true,"loggedIn":true,"user":"ana","adminOrgs":["FORTGROUND","outra"]},
     "current":{"org":"FORTGROUND","machine":"pc","agent":"codex","reviewAgent":"claude","dockerInRunner":true,"runnersRunning":0},
     "saved":{"claude":false,"openai":false,"tracker":true,"codexLogin":false},
     "suggestion":{"runners":4,"max":4,"perRunnerCpus":3,"perRunnerGb":6}}"#;

    #[test]
    fn the_check_is_read_even_after_noise_from_the_shell() {
        let check = parse_check(CHECK).unwrap();
        assert!(check.ready_for_form());
        assert_eq!(check.docker.cpus, 24);
        assert_eq!(check.gh.admin_orgs, ["FORTGROUND", "outra"]);
        let answers = Answers::from_check(&check);
        assert_eq!((answers.agent, answers.review_agent, answers.runners), (Agent::Codex, Agent::Claude, 4));
        assert!(parse_check("bash: setup.sh: não encontrado").is_err());
    }

    #[test]
    fn missing_tools_keep_the_form_closed() {
        let check = parse_check(r#"{"missingTools":["jq","gh"],"os":"Linux"}"#).unwrap();
        assert!(!check.ready_for_form());
        assert_eq!(check.missing_tools, ["jq", "gh"]);
    }

    #[test]
    fn each_agent_asks_only_for_its_own_credential() {
        let check = parse_check(CHECK).unwrap();
        let mut a = Answers::from_check(&check);
        // Codex corrige pela assinatura e Claude revisa: faltam as duas contas.
        assert_eq!(a.problems(&check), ["Conecte a conta do Claude.", "Conecte a conta do ChatGPT."]);
        a.claude = " sk-ant-oat-1 \n".into();
        assert_eq!(a.problems(&check), ["Conecte a conta do ChatGPT."]);
        a.codex_login = CodexLogin::Api;
        assert_eq!(a.problems(&check), ["Falta a chave da API da OpenAI."]);
        a.review_agent = Agent::Codex;
        a.agent = Agent::Codex;
        a.openai = "sk-x".into();
        a.claude.clear();
        assert!(a.problems(&check).is_empty());
    }

    #[test]
    fn the_input_carries_one_answer_per_line_and_only_typed_secrets() {
        let check = parse_check(CHECK).unwrap();
        let mut a = Answers::from_check(&check);
        a.machine = "PC do\nTiago".into();
        a.claude = "sk-ant-oat 01\n".into();
        let input = a.to_input();
        assert!(input.contains("MACHINE=PC do Tiago\n"));
        assert!(input.contains("AGENT=codex\nREVIEW_AGENT=claude\n"));
        assert!(input.contains("CODEX_LOGIN=chatgpt\n"));
        assert!(input.contains("CLAUDE_TOKEN=sk-ant-oat01\n"));
        assert!(!input.contains("FG_TRACKER_TOKEN"), "token já gravado não é reenviado");
        assert!(!input.contains("OPENAI_API_KEY"));
        a.forget_secrets();
        assert!(a.claude.is_empty());
    }

    #[test]
    fn marks_codes_and_urls_are_recognized() {
        assert_eq!(LogLine::parse("::step::Gravando").unwrap().kind, LineKind::Step);
        let gh = LogLine::parse("::code::ABCD-1234").unwrap();
        assert_eq!((gh.kind, gh.text.as_str()), (LineKind::Code, "ABCD-1234"));
        let codex = LogLine::parse("   Enter this one-time code: 7KQX-M2PZ9").unwrap();
        assert_eq!((codex.kind, codex.text.as_str()), (LineKind::Code, "7KQX-M2PZ9"));
        assert_eq!(LogLine::parse("runner-runner1-1  Started").unwrap().kind, LineKind::Plain);
        assert_eq!(LogLine::parse("fg-factory-runner Built").unwrap().kind, LineKind::Plain);
        assert!(LogLine::parse("   ").is_none());
        assert_eq!(find_url("Abra https://auth.openai.com/codex/device.").as_deref(), Some("https://auth.openai.com/codex/device"));
        assert_eq!(find_code("2026-11-12"), None);
    }

    #[test]
    fn the_claude_login_gives_the_address_then_the_token_only_when_complete() {
        let url = "https://claude.com/cai/oauth/authorize?code=true&client_id=x&scope=user%3Ainference&state=abc";
        let screen = format!("Welcome·Openingbrowser…Browser didn't open?Use the url below{url}\r\nPastecodehereifprompted>");
        assert_eq!(claude_login(&screen, false), (Some(url.to_owned()), None));
        // Endereço cortado no fim do pedaço: espera o resto.
        assert_eq!(claude_login(&url[..40], false).0, None);
        let token = format!("sk-ant-oat01-{}", "A1b_2-".repeat(12));
        assert_eq!(claude_login(&format!("{screen}\r\nYour token: {token}\r\n"), false).1, Some(token.clone()));
        assert_eq!(claude_login(&format!("Your token: {}", &token[..30]), false).1, None);
        assert_eq!(claude_login(&format!("Your token: {token}"), true).1, Some(token));
    }

    #[test]
    fn each_login_is_required_only_for_the_agent_that_uses_it() {
        let check = parse_check(CHECK).unwrap();
        let mut a = Answers::from_check(&check);
        a.claude = "sk-ant-oat01-x".into();
        // Codex pela assinatura sem login: falta conectar.
        assert_eq!(a.problems(&check), ["Conecte a conta do ChatGPT."]);
        let mut logged = check.clone();
        logged.saved.codex_login = true;
        assert!(a.problems(&logged).is_empty());
    }
}

/// O que os runners desta máquina estão fazendo (`setup.sh --status`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StatusPage {
    pub org: String,
    pub runners: Vec<RunnerStatus>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RunnerStatus {
    pub service: String,
    pub name: String,
    pub running: bool,
    pub online: bool,
    /// paused, idle, working ou offline.
    pub state: String,
    pub repository: String,
    pub issue: Option<u64>,
    pub title: String,
    /// setup, agent, validate, review ou publish.
    pub step: String,
    pub run_id: String,
    pub agent: String,
    pub review_agent: String,
    pub log_lines: u64,
    /// O que impede o runner de trabalhar (credenciais, token recusado pelo painel).
    pub problems: Vec<String>,
}

impl RunnerStatus {
    pub fn state_label(&self) -> &'static str {
        match self.state.as_str() {
            "paused" => "Pausado",
            "working" => "Trabalhando",
            "idle" => "Livre",
            "attention" => "Precisa de configuração",
            _ => "Sem sinal do GitHub",
        }
    }

    /// A etapa da tarefa, com o agente que roda nela.
    pub fn step_label(&self) -> Option<String> {
        let agent = |a: &str| if a == "codex" { "Codex" } else { "Claude Code" };
        Some(match self.step.as_str() {
            "setup" => "Preparando o repositório".to_owned(),
            "agent" => format!("Corrigindo ({})", agent(&self.agent)),
            "validate" => "Validando a correção".to_owned(),
            "review" => format!("Revisando ({})", agent(&self.review_agent)),
            "publish" => "Publicando".to_owned(),
            _ => return None,
        })
    }

    pub fn run_url(&self) -> Option<String> {
        (!self.repository.is_empty() && !self.run_id.is_empty())
            .then(|| format!("https://github.com/{}/actions/runs/{}", self.repository, self.run_id))
    }
}

/// Os repositórios com o workflow da factory (`setup.sh --repos`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RepoList {
    pub org: String,
    pub repos: Vec<RepoAuto>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RepoAuto {
    pub repository: String,
    /// main (o workflow está na branch principal) ou pr (ainda no pull request).
    pub workflow: String,
    pub auto: bool,
    /// A configuração da IA gravada no repositório (FACTORY_SETTINGS); `{}` = a central.
    pub settings: serde_json::Value,
}

/// Modelo, raciocínio e modo rápido de uma etapa, no Claude Code e no Codex. Vazio = não define
/// aqui (vale a configuração central, a da máquina ou, no revisor, a de quem corrige).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiForm {
    pub model: String,
    pub effort: String,
    pub fast: Option<bool>,
    pub codex_model: String,
    pub codex_effort: String,
    pub codex_fast: Option<bool>,
}

/// A configuração da IA de um repositório, como o formulário da aba Repositórios a edita.
#[derive(Clone, Debug, PartialEq)]
pub struct RepoForm {
    /// "" (o que a máquina declarar), "claude" ou "codex".
    pub agent: String,
    pub fix: AiForm,
    pub review_enabled: bool,
    /// "" (o mesmo de quem corrige), "claude" ou "codex".
    pub review_agent: String,
    pub review: AiForm,
    pub merge: bool,
}

impl Default for RepoForm {
    fn default() -> Self {
        RepoForm { agent: String::new(), fix: AiForm::default(), review_enabled: true, review_agent: String::new(), review: AiForm::default(), merge: false }
    }
}

fn text(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or_default().to_owned()
}

impl AiForm {
    fn from_json(v: &serde_json::Value) -> Self {
        let codex = v.get("codex").cloned().unwrap_or_default();
        AiForm {
            model: text(v, "model"),
            effort: text(v, "effort"),
            fast: v.get("fast").and_then(|x| x.as_bool()),
            codex_model: text(&codex, "model"),
            codex_effort: text(&codex, "effort"),
            codex_fast: codex.get("fast").and_then(|x| x.as_bool()),
        }
    }
    fn write(&self, out: &mut serde_json::Map<String, serde_json::Value>) {
        use serde_json::{Value, json};
        let put = |out: &mut serde_json::Map<String, Value>, key: &str, value: &str| {
            if !value.trim().is_empty() {
                out.insert(key.into(), json!(value.trim()));
            }
        };
        put(out, "model", &self.model);
        put(out, "effort", &self.effort);
        if let Some(fast) = self.fast {
            out.insert("fast".into(), json!(fast));
        }
        let mut codex = serde_json::Map::new();
        put(&mut codex, "model", &self.codex_model);
        put(&mut codex, "effort", &self.codex_effort);
        if let Some(fast) = self.codex_fast {
            codex.insert("fast".into(), json!(fast));
        }
        if !codex.is_empty() {
            out.insert("codex".into(), Value::Object(codex));
        }
    }
}

impl RepoForm {
    pub fn from_json(v: &serde_json::Value) -> Self {
        let review = v.get("review").cloned().unwrap_or_default();
        RepoForm {
            agent: text(v, "agent"),
            fix: AiForm::from_json(v),
            review_enabled: review.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true),
            review_agent: text(&review, "agent"),
            review: AiForm::from_json(&review),
            merge: v.get("merge").and_then(|x| x.as_bool()).unwrap_or(false),
        }
    }

    /// Só o que difere do padrão vai para a variável: o resto continua vindo da configuração central.
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::{Value, json};
        let mut out = serde_json::Map::new();
        if !self.agent.is_empty() {
            out.insert("agent".into(), json!(self.agent));
        }
        self.fix.write(&mut out);
        let mut review = serde_json::Map::new();
        if !self.review_enabled {
            review.insert("enabled".into(), json!(false));
        }
        if !self.review_agent.is_empty() {
            review.insert("agent".into(), json!(self.review_agent));
        }
        self.review.write(&mut review);
        if !review.is_empty() {
            out.insert("review".into(), Value::Object(review));
        }
        if self.merge {
            out.insert("merge".into(), json!(true));
        }
        Value::Object(out)
    }
}

pub fn parse_repos(text: &str) -> Result<RepoList, String> {
    let start = text.find('{').ok_or_else(|| format!("O setup.sh não respondeu como esperado:\n{}", text.trim()))?;
    serde_json::from_str(&text[start..]).map_err(|e| format!("Resposta do setup.sh ilegível ({e})."))
}

pub fn parse_status(text: &str) -> Result<StatusPage, String> {
    let start = text.find('{').ok_or_else(|| format!("O setup.sh não respondeu como esperado:\n{}", text.trim()))?;
    serde_json::from_str(&text[start..]).map_err(|e| format!("Resposta do setup.sh ilegível ({e})."))
}

#[cfg(test)]
mod status_tests {
    use super::*;

    #[test]
    fn the_status_says_what_each_runner_does() {
        let page = parse_status(r#"{"org":"FORTGROUND","runners":[
          {"service":"runner1","name":"factory-pc-1","running":true,"online":true,"state":"working","busy":true,
           "repository":"FORTGROUND/gallery-backend","issue":42,"title":"Erro 500","step":"review","runId":"987","agent":"codex","reviewAgent":"claude","logLines":4},
          {"service":"runner2","name":"factory-pc-2","running":false,"online":false,"state":"paused"}]}"#).unwrap();
        let (working, paused) = (&page.runners[0], &page.runners[1]);
        assert_eq!(working.state_label(), "Trabalhando");
        assert_eq!(working.step_label().as_deref(), Some("Revisando (Claude Code)"));
        assert_eq!(working.run_url().as_deref(), Some("https://github.com/FORTGROUND/gallery-backend/actions/runs/987"));
        assert_eq!((paused.state_label(), paused.step_label(), paused.run_url()), ("Pausado", None, None));
        assert!(parse_status("erro").is_err());
        let attention = parse_status(r#"{"runners":[{"service":"runner1","running":true,"state":"attention","problems":["sem as credenciais guardadas"]}]}"#).unwrap();
        assert_eq!(attention.runners[0].state_label(), "Precisa de configuração");
        assert_eq!(attention.runners[0].problems, ["sem as credenciais guardadas"]);
        let form = RepoForm::from_json(&serde_json::json!({"agent": "codex", "codex": {"model": "gpt-x", "fast": true},
            "review": {"agent": "claude", "model": "opus", "effort": "xhigh"}, "merge": true}));
        assert_eq!((form.agent.as_str(), form.fix.codex_model.as_str(), form.fix.codex_fast), ("codex", "gpt-x", Some(true)));
        assert_eq!((form.review_agent.as_str(), form.review.model.as_str(), form.review_enabled, form.merge), ("claude", "opus", true, true));
        assert_eq!(RepoForm::from_json(&form.to_json()), form, "ida e volta");
        assert_eq!(RepoForm::default().to_json(), serde_json::json!({}), "o padrão não grava nada");
        let mut off = RepoForm::default();
        off.review_enabled = false;
        off.fix.model = "  ".into();
        assert_eq!(off.to_json(), serde_json::json!({"review": {"enabled": false}}));
        let repos = parse_repos(r#"{"org":"FORTGROUND","repos":[{"repository":"FORTGROUND/a","workflow":"main","auto":true},{"repository":"FORTGROUND/b","workflow":"pr","auto":false}]}"#).unwrap();
        assert_eq!((repos.repos[0].auto, repos.repos[1].workflow.as_str()), (true, "pr"));
    }
}
