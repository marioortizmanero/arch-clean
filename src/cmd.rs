use crate::Config;

use std::collections::HashSet;
use std::env;
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

/// Will only work for pacman v5.2.0+
pub fn last_installed(config: &Config) -> Result<Output> {
    // Represents an entry in the Pacman logs
    struct LogEntry {
        time: String,
        action: String,
        pkg: String,
        version: String,
    }

    // First obtaining all installed packages
    let cmd = Command::new("pacman").arg("-Qqe").output()?;
    let stdout = String::from_utf8(cmd.stdout)?;
    let installed = stdout.lines().collect::<HashSet<_>>();

    // To find unique package entries
    let mut unique = HashSet::new();

    // Then iterating the logs, from the bottom to the top
    let file = File::open(PACMAN_LOG)?;
    let reader = BufReader::new(file);
    let lines = reader.lines().collect::<Vec<_>>();
    let content = lines
        .into_iter()
        .rev()
        .filter_map(|line| {
            // Reading the relevant columns
            let line = line.ok()?;
            let mut params = line.split_whitespace();

            Some(LogEntry {
                time: params.next()?.to_string(),
                action: params.nth(1)?.to_string(),
                pkg: params.next()?.to_string(),
                version: params.next()?.to_string(),
            })
        })
        .filter(|e| e.action == "installed") // Only installations
        .filter(|e| installed.contains(e.pkg.as_str())) // Only still installed packages
        .filter(|e| unique.insert(e.pkg.clone())) // Unique
        .map(|e| format!("{} {} {}", e.time, e.pkg, e.version))
        .take(config.max_packages)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Output {
        title: format!(
            "Last {} explicitly installed packages [yay -Rns <pkg>]",
            config.max_packages
        ),
        content,
    })
}

pub fn orphans(_config: &Config) -> Result<Output> {
    let cmd = Command::new("pacman").arg("-Qqtd").output()?;
    let content = String::from_utf8(cmd.stdout)?;

    Ok(Output {
        title: "Orphan packages [yay -Rns <pkg>]".to_string(),
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
        title: "Cache cleaning [paccache -r, yay -Sc]".to_string(),
        content,
    })
}

pub fn trash_size(_config: &Config) -> Result<Output> {
    let cmd = Command::new("du")
        .arg("-hs")
        .arg(env::var("HOME").unwrap() + "/.local/share/Trash")
        .output()?;
    let content = String::from_utf8(cmd.stdout)?;

    Ok(Output {
        title: "Trash size [trash-empty]".to_string(),
        content,
    })
}

pub fn dev_updates(_config: &Config) -> Result<Output> {
    let cmd = Command::new("yay")
        .arg("-Sua")
        .arg("--confirm")
        .arg("--devel")
        .stdin(Stdio::null()) // EOF for "dry run"
        .output()?;
    let stdout = String::from_utf8(cmd.stdout)?;
    let content = stdout
        .lines()
        .filter(|line| line.to_string().contains("devel/"))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Output {
        title: "Developer updates [yay -Syu --devel]".to_string(),
        content,
    })
}

pub fn nvim_swap_files(_config: &Config) -> Result<Output> {
    let swap_dir = env::var("HOME").unwrap() + "/.local/share/nvim/swap";
    let count = fs::read_dir(&swap_dir)?.count();

    Ok(Output {
        title: format!("NeoVim swap files [rm {}/*]", swap_dir),
        content: format!("{} files", count),
    })
}

pub fn disk_usage(config: &Config) -> Result<Output> {
    // Will only show the sizes of the directories in the current path.
    let home = env::var("HOME").unwrap();
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
        title: "Disk usage distribution in home directory".to_string(),
        content: out,
    })
}

pub fn rust_target(_config: &Config) -> Result<Output> {
    // First finding all Rust projects
    let cmd = Command::new("find")
        .arg(env::var("HOME").unwrap())
        .arg("-name")
        .arg("Cargo.toml")
        .arg("-type")
        .arg("f") // In those directories with a `Cargo.toml` file
        .arg("-not")
        .arg("-path")
        .arg("*/\\.*") // That aren't in hidden dirs like `.cache`
        .arg("-exec")
        .arg("dirname")
        .arg("{}")
        .arg(";")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    let dirs = String::from_utf8(cmd.stdout)?;

    // Then looking for the `target` directories
    let mut total_kb = 0;
    for dir in dirs.lines() {
        let cmd = Command::new("find")
            .arg(dir)
            .arg("-name")
            .arg("target")
            .arg("-type")
            .arg("d") // In those directories with a `Cargo.toml` file
            .arg("-exec")
            .arg("du")
            .arg("-s")
            .arg("{}") // Get the size of the target directory
            .arg(";")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()?;

        // Sum the kilobytes of each directory
        let stdout = String::from_utf8(cmd.stdout)?;
        let dir_kb: i32 = stdout
            .lines()
            .map(|line| {
                let mut fields = line.split_whitespace();
                match fields.next() {
                    Some(kb) => kb.parse().unwrap_or(0),
                    None => 0,
                }
            })
            .sum();
        total_kb += dir_kb;
    }


    Ok(Output {
        title: "Size of Rust target directories [cargo-clean]".to_string(),
        content: format!("{} MB", total_kb / 1024)
    })
}
