mod cmd;

use cmd::Output;

use std::sync::{mpsc, Arc};
use std::thread;

use anyhow::Result;
use argh::FromArgs;

#[derive(FromArgs)]
/// Clean up your Arch installation, real fast.
///
/// Output format: "name [suggestion]".
pub struct Config {
    /// maximum of explicitly installed packages to be shown
    #[argh(option, default = "10")]
    max_packages: usize,

    /// maximum of disk usage entries to be shown
    #[argh(option, default = "10")]
    max_disk_usage: usize,
}

fn main() {
    // The commands are accompanied by their titles and a suggested fix between
    // parenthesis.
    let cmds: &[fn(&Config) -> Result<Output>] = &[
        cmd::last_installed,
        cmd::orphans,
        cmd::paccache,
        cmd::trash_size,
        cmd::disk_usage,
        cmd::dev_updates,
        cmd::nvim_swap_files,
        cmd::rust_target,
    ];

    // Quick config with argh
    let conf = Arc::new(argh::from_env());

    // A group of threads with the processes
    let (wr, rd) = mpsc::channel();
    let mut handles = Vec::new();
    for cmd in cmds.iter() {
        let wr = wr.clone();
        let conf = Arc::clone(&conf);
        handles.push(thread::spawn(move || {
            let output = cmd(&conf);
            wr.send(output).unwrap();
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
        handle.join().unwrap();
    }
}
