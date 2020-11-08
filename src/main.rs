mod cmd;

use cmd::Output;

use std::sync::{mpsc, Arc};
use std::thread;

use anyhow::Result;
use argh::FromArgs;

#[derive(FromArgs)]
/// Clean up your Arch installation, real fast.
pub struct Config {
    /// maximum of explicitly installed packages to be shown
    #[argh(option, default = "10")]
    max_packages: i32,
}

fn main() {
    // The commands are accompanied by their titles and a suggested fix between
    // parenthesis.
    let cmds: Vec<fn(&Config) -> Result<Output>> = vec![
        cmd::last_installed,
        cmd::orphan,
        cmd::paccache,
        cmd::trash_size,
        cmd::devel_updates,
        cmd::nvim_swap_files,
    ];

    // Quick config with argh
    let conf: Arc<Config> = Arc::new(argh::from_env());

    // A group of threads with the processes
    let (wr, rd) = mpsc::channel();
    let mut handles = Vec::new();
    for cmd in cmds {
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
                println!("{}", out.content);
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
