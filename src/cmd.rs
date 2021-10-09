use crate::Config;

use std::{collections::HashSet, convert::TryInto, env, fmt, path::PathBuf, process::Stdio};

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

impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fix = if self.fix_available {
            " (fix available)"
        } else {
            ""
        };
        writeln!(f, "\x1b[36;1m{}{}:\x1b[0m", self.title, fix)?;
        writeln!(f, "{}", self.content.trim())
    }
}

#[async_trait]
pub trait CleanupCommand: Sync + Send {
    /// Runs the command and checks the output
    async fn check(&mut self, config: &Config) -> Result<Output>;

    /// Non-blocking, this will just show the user what `apply_fix` does. By
    /// default it's nothing.
    fn show_fix(&self, _config: &Config) {}

    /// Applies the suggested fix for the command. By default it's nothing.
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
            title: format!("Last {} explicitly installed packages", config.max_packages),
            content,
            fix_available: false,
        })
    }
}

#[derive(Default)]
pub struct OrphanPackages {
    pkgs: Vec<String>,
}
#[async_trait]
impl CleanupCommand for OrphanPackages {
    async fn check(&mut self, _config: &Config) -> Result<Output> {
        let cmd = Command::new("pacman").arg("-Qqtd").output().await?;
        let mut content = String::from_utf8(cmd.stdout)?;
        self.pkgs = content.lines().map(ToString::to_string).collect();
        // Default message instead of empty string
        if content.is_empty() {
            content.push_str("(none)");
        }

        Ok(Output {
            title: "Orphan packages".to_string(),
            content,
            fix_available: !self.pkgs.is_empty(),
        })
    }

    fn show_fix(&self, _config: &Config) {
        let pkgs = self.pkgs.join(" ");
        println!("This fix will run the command:");
        println!("  yay -Rns --noconfirm {}", pkgs);
    }

    async fn apply_fix(&self, _config: &Config) -> Result<()> {
        Command::new("yay")
            .arg("-Rns")
            .arg("--noconfirm")
            .args(&self.pkgs)
            .output()
            .await?;

        Ok(())
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
        let fix_available = content.lines().count() != 1;

        Ok(Output {
            title: "Cache cleaning".to_string(),
            content,
            fix_available,
        })
    }
}

/// TODO: yay cache
/// yay -Sc
///
/// /var/cache/pacman/pkg/ -- cache
/// /var/lib/pacman/ -- repos
/// /home/mario/.cache/yay -- build

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
        // The trash can be emptied only when the size shown by du is other than
        // zero.
        let empty_trash = matches!(content.split_whitespace().next(), Some("0"));

        Ok(Output {
            title: "Trash size".to_string(),
            content,
            fix_available: !empty_trash,
        })
    }

    fn show_fix(&self, _config: &Config) {
        println!("This fix will run the command 'trash-empty'");
    }

    async fn apply_fix(&self, _config: &Config) -> Result<()> {
        Command::new("trash-empty").output().await?;

        Ok(())
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
        let mut content = stdout
            .lines()
            .filter(|line| line.to_string().contains("devel/"))
            .collect::<Vec<_>>()
            .join("\n");
        let fix_available = content.lines().count() > 0;
        // Default message instead of empty string
        if content.is_empty() {
            content.push_str("(none)");
        }

        Ok(Output {
            title: "Developer updates".to_string(),
            content,
            fix_available,
        })
    }

    fn show_fix(&self, _config: &Config) {
        println!("This fix will run the command 'yay -Syu --devel'");
    }

    async fn apply_fix(&self, _config: &Config) -> Result<()> {
        let mut cmd = Command::new("yay").arg("-Syu").arg("--devel").spawn()?;
        cmd.wait().await?;

        Ok(())
    }
}

#[derive(Default)]
pub struct NeovimSwapFiles {
    swap_dir: String,
}
#[async_trait]
impl CleanupCommand for NeovimSwapFiles {
    async fn check(&mut self, _config: &Config) -> Result<Output> {
        self.swap_dir = env::var("HOME").unwrap() + "/.local/share/nvim/swap";
        let count = match fs::read_dir(&self.swap_dir).await {
            Err(_) => 0,
            Ok(dir) => ReadDirStream::new(dir).fold(0, |acc, _| acc + 1).await, // No `.count` available yet
        };

        Ok(Output {
            title: "NeoVim swap files".to_owned(),
            content: format!("{} files", count),
            fix_available: count > 0,
        })
    }

    fn show_fix(&self, _config: &Config) {
        println!("This fix will remove the directory '{}'", self.swap_dir);
    }

    async fn apply_fix(&self, _config: &Config) -> Result<()> {
        fs::remove_dir_all(&self.swap_dir).await?;

        Ok(())
    }
}

#[derive(Default)]
pub struct DiskUsage;
#[async_trait]
impl CleanupCommand for DiskUsage {
    async fn check(&mut self, config: &Config) -> Result<Output> {
        // Will only show the sizes of the nodes in the user's home.
        let home = PathBuf::from(env::var("HOME").unwrap());
        let nodes = ReadDirStream::new(fs::read_dir(&home).await?)
            .map(|node| {
                node.map(|dir| dir.file_name())
                    .map(|name| [home.clone(), name.into()].iter().collect())
            })
            .collect::<std::io::Result<Vec<PathBuf>>>()
            .await?;

        let mut cmd = Command::new("du")
            .arg("-sch")
            .args(&nodes)
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
                .arg("d")
                .arg("-exec")
                .arg("du")
                .arg("-s")
                .arg("{}")
                .arg(";")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
                .await?;

            // Sum the kilobytes of each directory
            let stdout = String::from_utf8(cmd.stdout)?;
            let output = stdout
                .lines()
                .map(|line| match line.split_once('\t') {
                    Some((kb, path)) => (kb.parse().unwrap_or(0), PathBuf::from(path)),
                    None => panic!("unexpected output from `du`: {}", line),
                })
                .filter(|(ref kb, _)| kb > &0)
                .collect::<Vec<_>>();

            // If it's not empty, insert the directories into the list and add
            // to the total size
            if !output.is_empty() {
                let dir_kb: i32 = output.iter().map(|(kb, _)| kb).sum();
                total_kb += dir_kb;

                self.dirs.extend(output.into_iter().map(|(_, path)| path));
            }
        }

        Ok(Output {
            title: "Size of Rust target directories".to_string(),
            content: format!("{} MB", total_kb / 1024),
            fix_available: true,
        })
    }

    fn show_fix(&self, _config: &Config) {
        println!("This fix will remove the following directories:");
        for dir in &self.dirs {
            println!("* {}", dir.to_str().unwrap());
        }
    }

    async fn apply_fix(&self, _config: &Config) -> Result<()> {
        for dir in &self.dirs {
            fs::remove_dir_all(dir).await?
        }

        Ok(())
    }
}
