//! A Zed extension that runs `fastanim lsp` for Rhai files.

use zed_extension_api::{self as zed, LanguageServerId, Result};

struct Fastanim;

impl zed::Extension for Fastanim {
    fn new() -> Self {
        Fastanim
    }

    fn language_server_command(
        &mut self,
        _: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let command = worktree
            .which("fastanim")
            .ok_or("`fastanim` is not on PATH; see the fastanim book's Install section")?;
        Ok(zed::Command {
            command,
            args: vec!["lsp".into()],
            env: vec![],
        })
    }
}

zed::register_extension!(Fastanim);
