mod cmd;

use cmd::{CleanupCommand, Output};

use std::{
    io::{self, Write},
    sync::Arc,
};

use anyhow::Result;
use argh::FromArgs;
use tokio::{sync::mpsc, task};

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

// #[derive(Debug)]
pub struct CheckDone {
    cmd: Box<dyn CleanupCommand>,
    output: Result<Output>,
}

impl std::fmt::Debug for Box<dyn CleanupCommand> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // The commands are accompanied by their titles and a suggested fix between
    // parenthesis.
    let cmds: [Box<dyn CleanupCommand>; 8] = [
        Box::new(cmd::LastInstalled::default()),
        Box::new(cmd::OrphanPackages::default()),
        Box::new(cmd::Paccache::default()),
        Box::new(cmd::TrashSize::default()),
        Box::new(cmd::DiskUsage::default()),
        Box::new(cmd::DevUpdates::default()),
        Box::new(cmd::NeovimSwapFiles::default()),
        Box::new(cmd::RustTarget::default()),
    ];
    let num_cmds = cmds.len();

    // Quick config with argh
    let conf = Arc::new(argh::from_env());

    // The check commands are each run in a separate task
    let (wr, mut rd) = mpsc::unbounded_channel();
    let mut handles = Vec::new();
    for mut cmd in cmds {
        let wr = wr.clone();
        let conf = Arc::clone(&conf);
        handles.push(task::spawn(async move {
            let output = cmd.check(&conf).await;
            wr.send((cmd, output)).unwrap();
        }));
    }
    drop(wr);

    // Synchonizing the results and saving them for later
    let mut checks = Vec::new();
    while let Some(msg) = rd.recv().await {
        checks.push(msg);

        // Report progress
        print!(".");
        io::stdout().flush().unwrap()
    }

    // Wait for any work left
    for handle in handles {
        handle.await.expect("Failed to join task");
    }

    // Applying the fix if indicated
    for (cmd, out) in checks {
        match out {
            Ok(out) => {
                let fix = if out.fix_available {
                    " (fix available)"
                } else {
                    ""
                };
                println!("\x1b[36m{}{}:\x1b[0m", out.title, fix);
                println!("{}\n", out.content.trim());
                if conf.apply && out.fix_available {
                    cmd.apply_fix(&conf).await.unwrap_or_else(|e| {
                        eprintln!("Failed to apply fix: {}", e);
                    })
                }
            }
            Err(e) => {
                eprintln!("Failed to run command: {}", e);
            }
        }
    }

    Ok(())
}
