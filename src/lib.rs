use std::{collections::HashMap, path::PathBuf};

use clap::Parser;
use log::error;
use theme::BuildOptions;

mod interpreter;
mod parser;
mod preprocess;
mod theme;

pub fn setup_logging() {
    use env_logger::Env;

    let env = Env::default().default_filter_or("reaper_theme_packer=warn");

    env_logger::init_from_env(env);
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct MainArgs {
    input: PathBuf,
    output: PathBuf,
    #[clap(long, short, action)]
    overwrite: bool,
}

pub fn main() {
    setup_logging();

    let args: MainArgs = MainArgs::parse();

    let theme_name = match args.output.file_stem() {
        None => return error!("output file does not have a name"),
        Some(stem) => match stem.to_str() {
            None => return error!("output file name is not valid UTF8"),
            Some(x) => x,
        },
    };

    let globals = {
        let mut map: HashMap<String, String> = HashMap::new();
        map.insert("THEME_NAME".into(), theme_name.to_string());
        map
    };
    let (rtconfig, reapertheme, resources) =
        match preprocess::preprocess(&args.input, Some(globals)) {
            Ok(x) => x,
            Err(err) => return error!("{}", err),
        };

    let theme = theme::Theme::new(theme_name, &rtconfig, reapertheme, resources);
    match theme.build(
        &args.output,
        &BuildOptions::default().overwrite(args.overwrite),
    ) {
        Err(err) => return error!("{}", err),
        _ => (),
    }
}
