use crate::Config;

use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::process::{Command, Stdio};

use anyhow::Result;

const PACMAN_LOG: &str = "/var/log/pacman.log";
const NVIM_SWAP_DIR: &str = "~/.local/share/nvim/swap";

pub fn last_installed(config: &Config) -> Result<String> {
    // First obtaining all installed packages
    let cmd = Command::new("pacman").arg("-Qqe").output()?;
    let installed = String::from_utf8(cmd.stdout)?
        .lines()
        .map(ToString::to_string)
        .collect::<HashSet<_>>();

    // Then reading the logs and showing the currently installed packages
    let file = File::open(PACMAN_LOG)?;
    let reader = BufReader::new(file);
    let out = reader
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
                return None;
            }

            // Only still installed packages
            if !installed.contains(&pkg) {
                return None;
            }

            Some(format!("{} {} {}", timestamp, pkg, version))
        })
        .take(config.max_packages as usize)
        .collect::<Vec<_>>()
        .join("\n");

    return Ok(out);
}

pub fn orphan(_config: &Config) -> Result<String> {
    let cmd = Command::new("pacman").arg("-Qqtd").output()?;
    let out = String::from_utf8(cmd.stdout)?;

    return Ok(out);
}

pub fn paccache(_config: &Config) -> Result<String> {
    let cmd = Command::new("paccache")
        .arg("-d")
        .arg("-v")
        .arg("--nocolor")
        .output()?;
    let out = String::from_utf8(cmd.stdout)?;

    return Ok(out);
}

pub fn trash_size(_config: &Config) -> Result<String> {
    let cmd = Command::new("du")
        .arg("-hs")
        .arg("~/.local/share/Trash")
        .output()?;
    let out = String::from_utf8(cmd.stdout)?;

    return Ok(out);
}

pub fn devel_updates(_config: &Config) -> Result<String> {
    let cmd = Command::new("yay")
        .arg("-Sua")
        .arg("--confirm")
        .arg("--devel")
        .stdin(Stdio::null()) // EOF for "dry run"
        .output()?;
    let stdout = String::from_utf8(cmd.stdout)?;
    let out = stdout
        .lines()
        .filter_map(|line| {
            if !line.to_string().contains("devel/") {
                return None;
            }
            Some(line)
        })
        .collect::<Vec<_>>()
        .join("\n");

    return Ok(out);
}

pub fn swap_files(_config: &Config) -> Result<String> {
    let count = fs::read_dir(NVIM_SWAP_DIR)?.count();

    return Ok(format!("{} files", count));
}
