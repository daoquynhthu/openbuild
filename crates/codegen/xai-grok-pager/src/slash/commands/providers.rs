use crate::app::actions::Action;
use crate::slash::command::{ArgItem, CommandExecCtx, CommandResult, SlashCommand};

pub struct ProvidersCommand;

impl SlashCommand for ProvidersCommand {
    fn name(&self) -> &str {
        "providers"
    }

    fn description(&self) -> &str {
        "List and configure model providers"
    }

    fn usage(&self) -> &str {
        "/providers [name]"
    }

    fn aliases(&self) -> &[&str] {
        &["provider"]
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn args_required(&self) -> bool {
        false
    }

    fn visible(&self, _ctx: &crate::slash::command::AppCtx) -> bool {
        true
    }

    fn session_scoped(&self) -> bool {
        false
    }

    fn offered_when_session_less(&self) -> bool {
        true
    }

    fn suggest_args(&self, _ctx: &crate::slash::command::AppCtx, _query: &str) -> Option<Vec<ArgItem>> {
        Some(vec![
            ArgItem {
                display: "xai".into(),
                match_text: "xai".into(),
                insert_text: "xai".into(),
                description: "xAI provider (grok)".into(),
            },
            ArgItem {
                display: "openai".into(),
                match_text: "openai".into(),
                insert_text: "openai".into(),
                description: "OpenAI provider".into(),
            },
            ArgItem {
                display: "anthropic".into(),
                match_text: "anthropic".into(),
                insert_text: "anthropic".into(),
                description: "Anthropic provider (Claude)".into(),
            },
            ArgItem {
                display: "opencode".into(),
                match_text: "opencode".into(),
                insert_text: "opencode".into(),
                description: "OpenCode Zen provider".into(),
            },
            ArgItem {
                display: "ollama".into(),
                match_text: "ollama".into(),
                insert_text: "ollama".into(),
                description: "Ollama local provider".into(),
            },
        ])
    }

    fn run(&self, _ctx: &mut CommandExecCtx, _args: &str) -> CommandResult {
        CommandResult::Action(Action::OpenProviders)
    }
}
