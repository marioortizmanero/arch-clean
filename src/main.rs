mod cmd;

use argh::FromArgs;
use anyhow::Result;

#[derive(FromArgs)]
/// Clean up your Arch installation, real fast.
pub struct Config {
    /// maximum of explicitly installed packages to be shown
    #[argh(option, default = "10")]
    max_packages: i32,
}

fn handle<T>(conf: &Config, cmd: T) where T: Fn(&Config) -> Result<cmd::Output> {
    let out = cmd(conf);
    match out {
        Ok(out) => {
            println!("\x1b[36m{}:\x1b[0m", out.title);
            println!("{}\n", out.content);
        },
        Err(err) =>  {
            eprintln!("Failed command: {}", err);
        }
    }
}

fn main() {
    let cmds: Vec<fn(&Config) -> Result<cmd::Output>> = vec![
        cmd::last_installed,
        cmd::orphan,
        cmd::paccache,
        cmd::trash_size,
        cmd::devel_updates,
        cmd::swap_files,
    ];
    let conf: Config = argh::from_env();

    // Launches the threads under the same scope
    crossbeam::scope(|s| {
        let conf = &conf;
        for cmd in cmds {
            s.spawn(move |_| handle(conf, cmd));
        }
    }).unwrap();
}
