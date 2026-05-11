mod args;
mod config;
mod files;
mod fixer;
mod render;
mod runner;
mod settings;

fn main() {
    if let Err(error) = runner::run() {
        eprintln!("pydocsync: {error}");
        std::process::exit(2);
    }
}
