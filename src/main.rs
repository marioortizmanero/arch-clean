mod cmd;

use cmd::CleanupCommand;

use std::{
    io::{self, Write},
    sync::Arc,
};

use anyhow::Result;
use argh::FromArgs;
use tokio::{sync::mpsc, task};

#[derive(FromArgs)]
/// Clean up your Arch installation, real fast.
pub struct Config {
    /// apply the suggested fix
    #[argh(switch)]
    apply: bool,

    /// maximum of explicitly installed packages to be shown
    #[argh(option, default = "10")]
    max_packages: usize,

    /// maximum of disk usage entries to be shown
    #[argh(option, default = "10")]
    max_disk_usage: usize,
}

impl std::fmt::Debug for Box<dyn CleanupCommand> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cleanup command")
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
    drop(wr); // The channel will be closed automatically

    // Synchonizing the results from the tasks
    while let Some((cmd, out)) = rd.recv().await {
        match out {
            Ok(out) => {
                // Printing the status
                let fix = if out.fix_available {
                    " (fix available)"
                } else {
                    ""
                };
                println!("\x1b[36;1m{}{}:\x1b[0m", out.title, fix);
                println!("{}\n", out.content.trim());

                // The fix will only be applied if it's configured and if the
                // command actually has a fix available
                if !conf.apply || !out.fix_available {
                    continue;
                }

                // The fix is a two-step process, first we make sure that
                // the user wants to continue
                cmd.show_fix(&conf);
                print!("\x1b[33mConfirm? [y/N]:\x1b[0m ");
                let mut confirm = String::new();
                io::stdout().flush()?;
                io::stdin().read_line(&mut confirm)?;
                if confirm.trim() != "y" {
                    println!("\x1b[31mSkipped\x1b[0m\n");
                    continue;
                }

                // They are applied sequentially so that the user sees the
                // results of the command.
                cmd.apply_fix(&conf).await.unwrap_or_else(|e| {
                    eprintln!("Failed to apply fix: {}", e);
                });
                println!("\x1b[32mDone\x1b[0m\n");
            }
            Err(e) => {
                eprintln!("Failed to run command: {}", e);
            }
        }
    }

    // Wait for any work left in the tasks, which should be none at this point
    // anyway.
    for handle in handles {
        handle.await.expect("Failed to join task");
    }

    Ok(())
}
