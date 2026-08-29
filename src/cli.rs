use std::path::PathBuf;

const MIN_UPDATE_MS: u64 = 100;
const MAX_UPDATE_MS: u64 = 86_400_000;

#[derive(Debug, Clone)]
pub enum Action {
    Help,
    Version { verbose: bool },
    DefaultConfig,
}

#[derive(Debug, Clone, Default)]
pub struct Cli {
    pub action: Option<Action>,
    pub debug: bool,
    pub force_utf: bool,
    pub low_color: bool,
    pub force_tty: Option<bool>,
    pub config_file: Option<PathBuf>,
    pub filter: Option<String>,
    pub preset: Option<u8>,
    pub themes_dir: Option<PathBuf>,
    pub update_ms: Option<u64>,
}

impl Cli {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut cli = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--default-config" => cli.action = Some(Action::DefaultConfig),
                "-h" | "--help" => cli.action = Some(Action::Help),
                "-v" | "-V" => cli.action = Some(Action::Version { verbose: false }),
                "--version" => cli.action = Some(Action::Version { verbose: true }),
                "-d" | "--debug" => cli.debug = true,
                "--force-utf" => cli.force_utf = true,
                "-l" | "--low-color" => cli.low_color = true,
                "-t" | "--tty" => set_tty(&mut cli, true)?,
                "--no-tty" => set_tty(&mut cli, false)?,
                "-c" | "--config" => {
                    let value = next_value(&mut args, "Config")?;
                    let path = PathBuf::from(value);
                    if path.is_dir() {
                        return Err("Config file can't be a directory".into());
                    }
                    cli.config_file = Some(path);
                }
                "-f" | "--filter" => cli.filter = Some(next_value(&mut args, "Filter")?),
                "-p" | "--preset" => {
                    let value = next_value(&mut args, "Preset")?;
                    cli.preset = Some(
                        value
                            .parse::<u8>()
                            .map_err(|_| "Preset must be a positive number")?
                            .min(9),
                    );
                }
                "--themes-dir" => {
                    let value = PathBuf::from(next_value(&mut args, "Themes directory")?);
                    if !value.is_dir() {
                        return Err("Themes directory does not exist or is not a directory".into());
                    }
                    cli.themes_dir = Some(value);
                }
                "-u" | "--update" => {
                    let value = next_value(&mut args, "Update")?;
                    cli.update_ms = Some(
                        value
                            .parse::<u64>()
                            .map_err(|_| "Update must be a positive number")?
                            .clamp(MIN_UPDATE_MS, MAX_UPDATE_MS),
                    );
                }
                _ => return Err(format!("Unknown argument '\x1b[33m{arg}\x1b[0m'")),
            }
        }
        Ok(cli)
    }
}

fn set_tty(cli: &mut Cli, value: bool) -> Result<(), String> {
    if cli.force_tty.is_some() {
        return Err("tty mode can't be set twice".into());
    }
    cli.force_tty = Some(value);
    Ok(())
}

fn next_value(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{name} requires an argument"))
}

pub fn print_version(verbose: bool) {
    println!("btoprs \x1b[1mv{}\x1b[0m", env!("CARGO_PKG_VERSION"));
    if verbose {
        println!("Compiled with: {}", env!("BTOPRS_RUSTC_VERSION"));
        println!(
            "Configured with: Cargo {}",
            env!("BTOPRS_BUILD_CONFIGURATION")
        );
    }
}

pub fn print_usage() {
    println!("\x1b[1;4mUsage:\x1b[0m \x1b[1mbtoprs\x1b[0m [OPTIONS]");
    println!();
    println!("\x1b[1;4mOptions:\x1b[0m");
    println!("  \x1b[1m-c, --config\x1b[0m <file>     Path to a config file");
    println!(
        "  \x1b[1m-d, --debug\x1b[0m             Start in debug mode with additional logs and metrics"
    );
    println!("  \x1b[1m-f, --filter\x1b[0m <filter>   Set an initial process filter");
    println!("  \x1b[1m    --force-utf\x1b[0m         Override automatic UTF locale detection");
    println!("  \x1b[1m-l, --low-color\x1b[0m         Disable true color, 256 colors only");
    println!("  \x1b[1m-p, --preset\x1b[0m <id>       Start with a preset (0-9)");
    println!(
        "  \x1b[1m-t, --tty\x1b[0m               Force tty mode with ANSI graph symbols and 16 colors only"
    );
    println!("  \x1b[1m    --themes-dir\x1b[0m <dir>  Path to a custom themes directory");
    println!("  \x1b[1m    --no-tty\x1b[0m            Force disable tty mode");
    println!("  \x1b[1m-u, --update\x1b[0m <ms>       Set an initial update rate in milliseconds");
    println!("  \x1b[1m    --default-config\x1b[0m    Print default config to standard output");
    println!("  \x1b[1m-h, --help\x1b[0m              Show this help message and exit");
    println!(
        "  \x1b[1m-V, --version\x1b[0m           Show a version message and exit (more with --version)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_interval_is_bounded_before_it_reaches_instant_arithmetic() {
        let minimum = Cli::parse(["--update".into(), "1".into()]).unwrap();
        assert_eq!(minimum.update_ms, Some(MIN_UPDATE_MS));

        let maximum = Cli::parse(["--update".into(), u64::MAX.to_string()]).unwrap();
        assert_eq!(maximum.update_ms, Some(MAX_UPDATE_MS));
    }
}
