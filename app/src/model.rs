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
    pub claude: bool,
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
            out.push("Falta o token do Claude Code.".into());
        }
        if self.uses(Agent::Codex) && self.codex_login == CodexLogin::Api && !check.saved.openai && self.openai.trim().is_empty() {
            out.push("Falta a chave da API da OpenAI.".into());
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
        // Codex corrige pela assinatura e Claude revisa: falta só o token do Claude.
        assert_eq!(a.problems(&check), ["Falta o token do Claude Code."]);
        a.claude = " sk-ant-oat-1 \n".into();
        assert!(a.problems(&check).is_empty());
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
}
