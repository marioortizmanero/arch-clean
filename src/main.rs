mod cmd;

use cmd::{CleanupCommand, Output};

use std::{
    sync::{mpsc, Arc},
    thread,
};

use anyhow::Result;
use argh::FromArgs;
use async_std::task;

#[derive(FromArgs)]
/// Clean up your Arch installation, real fast.
///
/// Output format: "name [suggestion]".
pub struct Config {
    /// apply the suggested fix
    #[argh(option, default = "false")]
    apply: bool,

    /// maximum of explicitly installed packages to be shown
    #[argh(option, default = "10")]
    max_packages: usize,

    /// maximum of disk usage entries to be shown
    #[argh(option, default = "10")]
    max_disk_usage: usize,
}

#[tokio::main]
async fn main() {
    // The commands are accompanied by their titles and a suggested fix between
    // parenthesis.
    let cmds: [Box<dyn CleanupCommand>; 2] = [
        // cmd::last_installed,
        // cmd::orphans,
        // cmd::paccache,
        // cmd::trash_size,
        Box::new(cmd::DiskUsage::default()),
        // cmd::dev_updates,
        // cmd::nvim_swap_files,
        Box::new(cmd::RustTarget::default()),
    ];

    // Quick config with argh
    let conf = Arc::new(argh::from_env());

    // A group of threads with the processes
    let (wr, rd) = mpsc::channel();
    let mut handles = Vec::new();
    for mut cmd in cmds {
        let wr = wr.clone();
        let conf = Arc::clone(&conf);
        handles.push(task::spawn(async move {
            let output = cmd.check(&conf);
            wr.send(output).unwrap();

            if conf.apply {
                cmd.apply(&conf);
            }
        }));
    }

    // Stdout synchronized output
    for _ in 0..handles.len() {
        let out = rd.recv().unwrap();
        match out {
            Ok(out) => {
                println!("\x1b[36m{}:\x1b[0m", out.title);
                println!("{}\n", out.content.trim());
            }
            Err(e) => {
                eprintln!("Failed to run command: {}", e);
            }
        }
    }

    // Wait for any work left
    for handle in handles {
        handle.await.expect("Failed to join task");
    }
}
