use std::collections::BTreeMap;
use zed::settings::{CommandSettings, LspSettings};
use zed_extension_api as zed;

struct NitrideExtension;

impl zed::Extension for NitrideExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;
        server_command(
            settings.binary,
            worktree.which("nitride"),
            worktree.shell_env(),
        )
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<Option<zed::serde_json::Value>> {
        // Use standard LSP token types by default. The VS Code palette tokens
        // and virtual library document provider are client-specific.
        Ok(
            LspSettings::for_worktree(language_server_id.as_ref(), worktree)?
                .initialization_options,
        )
    }
}

fn server_command(
    binary: Option<CommandSettings>,
    path_binary: Option<String>,
    shell_env: zed::EnvVars,
) -> zed::Result<zed::Command> {
    let binary = binary.unwrap_or(CommandSettings {
        path: None,
        arguments: None,
        env: None,
    });
    let command = binary.path.filter(|path| !path.trim().is_empty()).or(path_binary)
        .ok_or("Cannot find nitride in PATH. Install the Nitride CLI or set lsp.nitride.binary.path in Zed settings.")?;
    let mut env: BTreeMap<_, _> = shell_env.into_iter().collect();
    env.extend(binary.env.unwrap_or_default());
    Ok(zed::Command {
        command,
        args: binary.arguments.unwrap_or_else(|| vec!["lsp".into()]),
        env: env.into_iter().collect(),
    })
}

zed::register_extension!(NitrideExtension);

#[cfg(test)]
mod tests {
    use super::*;
    use zed::settings::CommandSettings;

    #[test]
    fn path_binary_runs_lsp_with_the_worktree_environment() {
        let command = server_command(
            None,
            Some("/tools/nitride".into()),
            vec![("PATH".into(), "/tools".into())],
        )
        .unwrap();
        assert_eq!(command.command, "/tools/nitride");
        assert_eq!(command.args, ["lsp"]);
        assert_eq!(command.env, [("PATH".into(), "/tools".into())]);
    }

    #[test]
    fn configured_binary_arguments_and_environment_take_precedence() {
        let binary = CommandSettings {
            path: Some("/custom/nitride".into()),
            arguments: Some(vec!["lsp".into(), "--debounce-ms".into(), "25".into()]),
            env: Some([("MODE".into(), "custom".into())].into()),
        };
        let command = server_command(
            Some(binary),
            Some("/tools/nitride".into()),
            vec![
                ("MODE".into(), "shell".into()),
                ("PATH".into(), "/tools".into()),
            ],
        )
        .unwrap();
        assert_eq!(command.command, "/custom/nitride");
        assert_eq!(command.args, ["lsp", "--debounce-ms", "25"]);
        assert!(command.env.contains(&("MODE".into(), "custom".into())));
        assert!(command.env.contains(&("PATH".into(), "/tools".into())));
        assert_eq!(command.env.len(), 2);
    }

    #[test]
    fn path_override_keeps_default_lsp_argument() {
        let binary = CommandSettings {
            path: Some("/custom/nitride".into()),
            arguments: None,
            env: None,
        };
        let command = server_command(Some(binary), None, vec![]).unwrap();
        assert_eq!(command.command, "/custom/nitride");
        assert_eq!(command.args, ["lsp"]);
    }

    #[test]
    fn explicit_empty_arguments_are_respected() {
        let binary = CommandSettings {
            path: None,
            arguments: Some(vec![]),
            env: None,
        };
        assert!(
            server_command(Some(binary), Some("nitride-wrapper".into()), vec![])
                .unwrap()
                .args
                .is_empty()
        );
    }

    #[test]
    fn missing_binary_explains_how_to_configure_it() {
        let error = server_command(None, None, vec![]).err().unwrap();
        assert!(error.contains("PATH"));
        assert!(error.contains("lsp.nitride.binary.path"));
    }
}
