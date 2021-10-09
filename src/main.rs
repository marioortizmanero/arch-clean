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

/// The fix is a two-step process, first we make sure that the user wants to
/// continue. This is a blocking operation.
fn prompt_user(conf: &Config, cmd: &dyn CleanupCommand) -> Result<bool> {
    cmd.show_fix(conf);
    print!("\x1b[33mConfirm? [y/N]:\x1b[0m ");
    let mut confirm = String::new();
    io::stdout().flush()?;
    io::stdin().read_line(&mut confirm)?;

    Ok(confirm.trim() == "y")
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
    let mut handles = Vec::with_capacity(cmds.len());
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
            Err(e) => eprintln!("Failed to run command: {}", e),
            Ok(out) => {
                println!("{}", out);

                // The fixes are applied sequentially so that the user sees the
                // results of the command. They will only be applied when
                // configured and if the command actually has a fix available
                if !conf.apply || !out.fix_available {
                    continue;
                }

                if !prompt_user(&conf, &*cmd)? {
                    println!("\x1b[31mSkipped\x1b[0m\n");
                    continue;
                }

                cmd.apply_fix(&conf).await.unwrap_or_else(|e| {
                    eprintln!("Failed to apply fix: {}", e);
                });
                println!("\x1b[32mDone\x1b[0m\n");
            }
        }
    }

    // Wait for any work left in the tasks, which should be none at this point
    // anyway.
    for handle in handles {
        handle.await?;
    }

    Ok(())
}
