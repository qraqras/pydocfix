mod args;
mod baseline;
mod config;
mod files;
mod fixer;
mod render;
mod runner;
mod settings;
mod suppression;

fn main() {
    if let Err(error) = runner::run() {
        eprintln!("pydocfix: {error}");
        std::process::exit(2);
    }
}
