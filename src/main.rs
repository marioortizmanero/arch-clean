mod cmd;

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

struct Output {
    title: String,
    content: String,
}

fn handle<T>(conf: &Config, wr: mpsc::Sender<Output>, title: &str, cmd: T)
where
    T: Fn(&Config) -> Result<String>,
{
    let out = cmd(conf).unwrap_or_else(|e| format!("Failed command: {}", e));
    wr.send(Output {
        title: title.to_string(),
        content: out,
    })
    .unwrap();
}

fn main() {
    let cmds: Vec<(&str, fn(&Config) -> Result<String>)> = vec![
        (
            "Last explicitly installed packages (yay -Rns <pkg>)",
            cmd::last_installed,
        ),
        ("Orphan packages (yay -Rns <pkg>)", cmd::orphan),
        ("Cache cleaning (yay -Syu --devel)", cmd::paccache),
        ("Trash size (trash-empty)", cmd::trash_size),
        ("Devel updates (yay -Syu --devel)", cmd::devel_updates),
        (
            "NeoVim swap files (rm ~/.local/share/nvim/swap/*)",
            cmd::swap_files,
        ),
    ];
    let conf: Arc<Config> = Arc::new(argh::from_env());

    let mut handles = Vec::new();
    let (wr, rd) = mpsc::channel();
    // A group of threads with the processes
    for (title, cmd) in cmds {
        let wr = wr.clone();
        let conf = Arc::clone(&conf);
        handles.push(thread::spawn(move || handle(&conf, wr, title, cmd)));
    }

    // Stdout synchronized output in a different thread
    handles.push(thread::spawn(move || loop {
        let out = rd.recv();
        match out {
            Ok(out) => {
                println!("\x1b[36m{}:\x1b[0m", out.title);
                println!("{}", out.content);
            }
            Err(_) => break,
        }
    }));

    // Waiting for all of them
    for handle in handles {
        handle.join().unwrap();
    }
}
