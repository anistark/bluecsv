use zed_extension_api::{self as zed, Command, LanguageServerId, Result, Worktree};

struct BluecsvExtension;

impl zed::Extension for BluecsvExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let path = worktree.which("bluecsv-ls").ok_or_else(|| {
            "bluecsv-ls not found on PATH. Install a release binary from \
             https://github.com/anistark/bluecsv/releases or build one with \
             `cargo install --path server/bluecsv-ls`."
                .to_string()
        })?;
        Ok(Command {
            command: path,
            args: Vec::new(),
            env: Vec::new(),
        })
    }
}

zed::register_extension!(BluecsvExtension);
