use crate::Config;

use std::{collections::HashSet, convert::TryInto, env, path::PathBuf, process::Stdio};

use anyhow::Result;
use async_trait::async_trait;
use tokio::{
    fs::{self, File},
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use tokio_stream::{
    wrappers::{LinesStream, ReadDirStream},
    StreamExt,
};

const PACMAN_LOG: &str = "/var/log/pacman.log";

#[derive(Default, Debug)]
pub struct Output {
    pub title: String,
    pub content: String,
    pub fix_available: bool,
}

#[async_trait]
pub trait CleanupCommand: Sync + Send {
    async fn check(&mut self, config: &Config) -> Result<Output>;

    async fn apply_fix(&self, _config: &Config) -> Result<()> {
        Ok(())
    }
}

#[derive(Default)]
pub struct LastInstalled;
#[async_trait]
impl CleanupCommand for LastInstalled {
    /// Will only work for pacman v5.2.0+
    async fn check(&mut self, config: &Config) -> Result<Output> {
        // Represents an entry in the Pacman logs
        struct LogEntry {
            time: String,
            action: String,
            pkg: String,
            version: String,
        }

        // First obtaining all installed packages
        let cmd = Command::new("pacman").arg("-Qqe").output().await?;
        let stdout = String::from_utf8(cmd.stdout)?;
        let installed = stdout.lines().collect::<HashSet<_>>();

        // To find unique package entries
        let mut unique = HashSet::new();

        // Then iterating the logs, from the bottom to the top
        let file = File::open(PACMAN_LOG).await?;
        let reader = BufReader::new(file);
        let lines = LinesStream::new(reader.lines()).collect::<Vec<_>>().await;
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
            fix_available: false,
        })
    }
}

#[derive(Default)]
pub struct OrphanPackages;
#[async_trait]
impl CleanupCommand for OrphanPackages {
    async fn check(&mut self, _config: &Config) -> Result<Output> {
        let cmd = Command::new("pacman").arg("-Qqtd").output().await?;
        let content = String::from_utf8(cmd.stdout)?;

        Ok(Output {
            title: "Orphan packages [yay -Rns <pkg>]".to_string(),
            content,
            fix_available: true,
        })
    }
}

#[derive(Default)]
pub struct Paccache;
#[async_trait]
impl CleanupCommand for Paccache {
    async fn check(&mut self, _config: &Config) -> Result<Output> {
        let cmd = Command::new("paccache")
            .arg("-d")
            .arg("-v")
            .arg("--nocolor")
            .output()
            .await?;
        let content = String::from_utf8(cmd.stdout)?;

        Ok(Output {
            title: "Cache cleaning [paccache -r, yay -Sc]".to_string(),
            content,
            fix_available: true,
        })
    }
}

#[derive(Default)]
pub struct TrashSize;
#[async_trait]
impl CleanupCommand for TrashSize {
    async fn check(&mut self, _config: &Config) -> Result<Output> {
        let cmd = Command::new("du")
            .arg("-hs")
            .arg(env::var("HOME").unwrap() + "/.local/share/Trash")
            .output()
            .await?;
        let content = String::from_utf8(cmd.stdout)?;

        Ok(Output {
            title: "Trash size [trash-empty]".to_string(),
            content,
            fix_available: true,
        })
    }
}

#[derive(Default)]
pub struct DevUpdates;
#[async_trait]
impl CleanupCommand for DevUpdates {
    async fn check(&mut self, _config: &Config) -> Result<Output> {
        let cmd = Command::new("yay")
            .arg("-Sua")
            .arg("--confirm")
            .arg("--devel")
            .stdin(Stdio::null()) // EOF for "dry run"
            .output()
            .await?;
        let stdout = String::from_utf8(cmd.stdout)?;
        let content = stdout
            .lines()
            .filter(|line| line.to_string().contains("devel/"))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(Output {
            title: "Developer updates [yay -Syu --devel]".to_string(),
            content,
            fix_available: true,
        })
    }
}

#[derive(Default)]
pub struct NeovimSwapFiles;
#[async_trait]
impl CleanupCommand for NeovimSwapFiles {
    async fn check(&mut self, _config: &Config) -> Result<Output> {
        let swap_dir = env::var("HOME").unwrap() + "/.local/share/nvim/swap";
        let count = ReadDirStream::new(fs::read_dir(&swap_dir).await?)
            .fold(0, |acc, _| acc + 1) // No `.count` available yet
            .await;

        Ok(Output {
            title: format!("NeoVim swap files [rm {}/*]", swap_dir),
            content: format!("{} files", count),
            fix_available: true,
        })
    }
}

#[derive(Default)]
pub struct DiskUsage;
#[async_trait]
impl CleanupCommand for DiskUsage {
    async fn check(&mut self, config: &Config) -> Result<Output> {
        // Will only show the sizes of the directories in the current path.
        let home = PathBuf::from(env::var("HOME").unwrap());
        let mut dirs = Vec::new();
        let mut stream = ReadDirStream::new(fs::read_dir(&home).await?);
        for dir in stream.next().await {
            let dir = dir?;
            if dir.file_type().await?.is_dir() {
                let mut name = home.clone();
                name.push(dir.file_name());
                dirs.push(name);
            }
        }

        let mut cmd = Command::new("du")
            .arg("-sch")
            .args(&dirs)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        cmd.wait().await?;
        let du_stdin: Stdio = cmd.stdout.take().unwrap().try_into().unwrap();
        let cmd = Command::new("sort")
            .arg("-rh")
            .stdin(du_stdin)
            .output()
            .await?;
        let out = String::from_utf8(cmd.stdout)?
            .lines()
            .take(config.max_disk_usage)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(Output {
            title: "Disk usage distribution in home directory".to_string(),
            content: out,
            fix_available: false,
        })
    }
}

#[derive(Default)]
pub struct RustTarget {
    dirs: Vec<PathBuf>,
}
#[async_trait]
impl CleanupCommand for RustTarget {
    async fn check(&mut self, _config: &Config) -> Result<Output> {
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
            .output()
            .await?;
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
                .output()
                .await?;

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

            // If it's not empty, add it to the list and add to the total size
            if dir_kb > 0 {
                self.dirs.push(PathBuf::from(dir));
                total_kb += dir_kb;
            }
        }

        Ok(Output {
            title: "Size of Rust target directories [cargo-clean]".to_string(),
            content: format!("{} MB", total_kb / 1024),
            fix_available: true,
        })
    }

    async fn apply_fix(&self, _config: &Config) -> Result<()> {
        for dir in &self.dirs {
            fs::remove_dir_all(dir).await?
        }

        Ok(())
    }
}
