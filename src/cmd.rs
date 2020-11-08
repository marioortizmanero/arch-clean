use crate::Config;

use std::process::{Command, Stdio};
use std::io::prelude::*;
use std::io::BufReader;
use std::fs;
use std::fs::File;
use std::collections::HashSet;

use anyhow::Result;

pub struct Output {
    pub title: String,
    pub content: String,
}

const PACMAN_LOG: &str = "/var/log/pacman.log";
const NVIM_SWAP_DIR: &str = "~/.local/share/nvim/swap";

pub fn last_installed(config: &Config) -> Result<Output> {
    // First obtaining all installed packages
    let cmd = Command::new("pacman")
        .arg("-Qqe")
        .output()?;
    let installed = String::from_utf8(cmd.stdout)?
        .lines()
        .map(ToString::to_string)
        .collect::<HashSet<_>>();

    // Then reading the logs and showing the currently installed packages
    let file = File::open(PACMAN_LOG)?;
    let reader = BufReader::new(file);
    let content = reader
        .lines()
        .filter_map(|line| {
            // Reading the relevant columns
            let line = line.ok()?;
            let params = line.split(' ').collect::<Vec<_>>();
            let timestamp = params.get(0)?;
            let action = params.get(2)?.to_string();
            let pkg = params.get(3)?.to_string();
            let version = params.get(4)?;

            // Only installations
            if action != "installed" {
                return None
            }

            // Only still installed packages
            if !installed.contains(&pkg) {
                return None
            }

            Some(format!("{} {} {}", timestamp, pkg, version))
        })
        .take(config.max_packages as usize)
        .collect::<Vec<_>>()
        .join("\n");

    return Ok(Output {
        title: format!("Last {} explicitly installed packages", config.max_packages),
        content
    })
}

pub fn orphan(_config: &Config) -> Result<Output> {
    let cmd = Command::new("pacman")
        .arg("-Qqtd")
        .output()?;
    let content = String::from_utf8(cmd.stdout)?;

    return Ok(Output {
        title: "Orphan packages (yay -Rns <pkg>)".to_string(),
        content
    })
}

pub fn paccache(_config: &Config) -> Result<Output> {
    let cmd = Command::new("paccache")
        .arg("-d")
        .arg("-v")
        .arg("--nocolor")
        .output()?;
    let content = String::from_utf8(cmd.stdout)?;

    return Ok(Output {
        title: "Cache cleaning (yay -Syu --devel)".to_string(),
        content
    })
}

pub fn trash_size(_config: &Config) -> Result<Output> {
    let cmd = Command::new("du")
        .arg("-hs")
        .arg("~/.local/share/Trash")
        .output()?;
    let content = String::from_utf8(cmd.stdout)?;

    return Ok(Output {
        title: "Trash size (trash-empty)".to_string(),
        content
    })
}

pub fn devel_updates(_config: &Config) -> Result<Output> {
    let cmd = Command::new("yay")
        .arg("-Sua")
        .arg("--confirm")
        .arg("--devel")
        .stdin(Stdio::null()) // EOF for "dry run"
        .output()?;
    let stdout = String::from_utf8(cmd.stdout)?;
    let content = stdout
        .lines()
        .filter_map(|line| {
            if !line.to_string().contains("devel/") {
                return None
            }
            Some(line)
        })
        .collect::<Vec<_>>()
        .join("\n");


    return Ok(Output {
        title: "Devel updates (yay -Syu --devel)".to_string(),
        content
    })
}

pub fn swap_files(_config: &Config) -> Result<Output> {
    let count = fs::read_dir(NVIM_SWAP_DIR)?.count();

    return Ok(Output {
        title: format!("Neovim swap files (rm {}/*)", NVIM_SWAP_DIR),
        content: format!("{} files", count)
    })
}
