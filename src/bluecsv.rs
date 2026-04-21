use std::fs;

use zed_extension_api::{
    self as zed, Architecture, Command, DownloadedFileType, LanguageServerId,
    LanguageServerInstallationStatus, Os, Result, Worktree,
};

const REPO: &str = "anistark/bluecsv";
const BINARY_NAME: &str = "bluecsv-ls";

struct BluecsvExtension;

impl BluecsvExtension {
    fn language_server_binary_path(
        &self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<String> {
        if let Some(path) = worktree.which(BINARY_NAME) {
            return Ok(path);
        }

        let version = env!("CARGO_PKG_VERSION");
        let tag = format!("v{version}");
        let target = target_triple()?;
        let asset_name = format!("{BINARY_NAME}-{target}.tar.gz");
        let install_dir = format!("{BINARY_NAME}-{version}");
        let binary_path = format!("{install_dir}/{BINARY_NAME}");

        if fs::metadata(&binary_path).is_ok_and(|m| m.is_file()) {
            return Ok(binary_path);
        }

        zed::set_language_server_installation_status(
            id,
            &LanguageServerInstallationStatus::Downloading,
        );

        let release = zed::github_release_by_tag_name(REPO, &tag).map_err(|e| {
            set_failed(id, &e);
            format!("failed to fetch release {tag} from {REPO}: {e}")
        })?;

        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| {
                let msg = format!("no asset {asset_name} in release {tag}");
                set_failed(id, &msg);
                msg
            })?;

        zed::download_file(
            &asset.download_url,
            &install_dir,
            DownloadedFileType::GzipTar,
        )
        .map_err(|e| {
            set_failed(id, &e);
            format!("failed to download {}: {e}", asset.download_url)
        })?;

        zed::make_file_executable(&binary_path).map_err(|e| {
            set_failed(id, &e);
            format!("failed to chmod {binary_path}: {e}")
        })?;

        zed::set_language_server_installation_status(id, &LanguageServerInstallationStatus::None);
        Ok(binary_path)
    }
}

fn target_triple() -> Result<&'static str> {
    let (os, arch) = zed::current_platform();
    match (os, arch) {
        (Os::Mac, Architecture::Aarch64) => Ok("aarch64-apple-darwin"),
        (Os::Mac, Architecture::X8664) => Ok("x86_64-apple-darwin"),
        (Os::Linux, Architecture::X8664) => Ok("x86_64-unknown-linux-gnu"),
        _ => Err("no prebuilt bluecsv-ls for this platform. \
                  Install from source with `cargo install bluecsv-ls` and put it on PATH."
            .into()),
    }
}

fn set_failed(id: &LanguageServerId, msg: &str) {
    zed::set_language_server_installation_status(
        id,
        &LanguageServerInstallationStatus::Failed(msg.to_string()),
    );
}

impl zed::Extension for BluecsvExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        Ok(Command {
            command: self.language_server_binary_path(id, worktree)?,
            args: Vec::new(),
            env: Vec::new(),
        })
    }
}

zed::register_extension!(BluecsvExtension);
