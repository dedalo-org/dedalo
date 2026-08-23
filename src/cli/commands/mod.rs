pub mod identity;
pub mod init;
pub mod ledger;
pub mod plan;
pub mod scan;
pub mod settle;
pub mod status;
pub mod verify;

use crate::Engine;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Open the engine for the repository the user pointed at.
pub fn engine(repo: Option<&PathBuf>) -> Result<Engine> {
    let start = match repo {
        Some(path) => path.clone(),
        None => std::env::current_dir().context("cannot read the current directory")?,
    };
    Engine::discover(&start).with_context(|| format!("in {}", start.display()))
}

/// Resolve the directory a command should act on, without needing a config.
pub fn workdir(repo: Option<&PathBuf>) -> Result<PathBuf> {
    match repo {
        Some(path) => Ok(path.clone()),
        None => std::env::current_dir().context("cannot read the current directory"),
    }
}

pub fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Relative path for display, falling back to the absolute one.
pub fn display_path(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}
