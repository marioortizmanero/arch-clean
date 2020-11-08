use crate::Config;

use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::process::{Command, Stdio};

use anyhow::Result;

const PACMAN_LOG: &str = "/var/log/pacman.log";

pub struct Output {
    pub title: String,
    pub content: String,
}

// Returns the home directory for the user executing the file
fn get_home() -> String {
    dirs::home_dir()
        .unwrap()
        .into_os_string()
        .into_string()
        .unwrap()
}

pub fn last_installed(config: &Config) -> Result<Output> {
    // First obtaining all installed packages
    let cmd = Command::new("pacman").arg("-Qqe").output()?;
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
                return None;
            }

            // Only still installed packages
            if !installed.contains(&pkg) {
                return None;
            }

            Some(format!("{} {} {}", timestamp, pkg, version))
        })
        .take(config.max_packages)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Output {
        title: format!(
            "Last {} explicitly installed packages (yay -Rns <pkg>)",
            config.max_packages
        ),
        content,
    })
}

pub fn orphan(_config: &Config) -> Result<Output> {
    let cmd = Command::new("pacman").arg("-Qqtd").output()?;
    let content = String::from_utf8(cmd.stdout)?;

    Ok(Output {
        title: "Orphan packages (yay -Rns <pkg>)".to_string(),
        content,
    })
}

pub fn paccache(_config: &Config) -> Result<Output> {
    let cmd = Command::new("paccache")
        .arg("-d")
        .arg("-v")
        .arg("--nocolor")
        .output()?;
    let content = String::from_utf8(cmd.stdout)?;

    Ok(Output {
        title: "Cache cleaning (paccache -r)".to_string(),
        content,
    })
}

pub fn trash_size(_config: &Config) -> Result<Output> {
    let cmd = Command::new("du")
        .arg("-hs")
        .arg(get_home() + "/.local/share/Trash")
        .output()?;
    let content = String::from_utf8(cmd.stdout)?;

    Ok(Output {
        title: "Trash size (trash-empty)".to_string(),
        content,
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
                return None;
            }
            Some(line)
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Output {
        title: "Devel updates (yay -Syu --devel)".to_string(),
        content,
    })
}

pub fn nvim_swap_files(_config: &Config) -> Result<Output> {
    let swap_dir = get_home() + "/.local/share/nvim/swap";
    let count = fs::read_dir(&swap_dir)?.count();

    Ok(Output {
        title: format!("NeoVim swap files (rm {}/*)", swap_dir),
        content: format!("{} files", count),
    })
}

pub fn disk_usage(config: &Config) -> Result<Output> {
    // Will only show the sizes of the directories in the current path.
    let home = get_home();
    let dirs = fs::read_dir(&home)?
        .filter_map(|node| {
            let node = node.ok()?;
            if node.file_type().ok()?.is_dir() {
                let name = node.file_name();
                Some(format!("{}/{}", home, name.to_str().unwrap()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut cmd = Command::new("du")
        .arg("-sch")
        .args(&dirs)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    cmd.wait()?;
    let cmd = Command::new("sort")
        .arg("-rh")
        .stdin(cmd.stdout.take().unwrap())
        .output()?;
    let out = String::from_utf8(cmd.stdout)?
        .lines()
        .take(config.max_disk_usage)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Output {
        title: format!("Disk usage distribution in home directory"),
        content: out,
    })
}
