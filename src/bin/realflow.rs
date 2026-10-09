use clap::Parser;
fn main() {
    if let Err(error) = realflow::cli::execute(realflow::cli::Cli::parse()) {
        eprintln!("ERROR: {error:#}");
        std::process::exit(2);
    }
}
