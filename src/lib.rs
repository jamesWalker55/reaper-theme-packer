use std::{path::PathBuf};



use clap::Parser;

mod interpreter;
mod parser;
mod preprocess;
mod theme;

pub fn setup_logging() {
    use env_logger::Env;

    let env = Env::default().default_filter_or("reaper_theme_builder_2=warn");

    env_logger::init_from_env(env);
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct MainArgs {
    input: PathBuf,
    output: PathBuf,
}

pub fn main() {
    setup_logging();

    let args: MainArgs = MainArgs::parse();

    dbg!(&args.input.join("apple"));

    // let theme = theme::Theme::new(
    //     "temp",
    //     "; this is rtconfig code",
    //     Ini::load_from_str("[config]\nhello=world\n# this is a comment :)))\ntest=123\nhash=#asd")
    //         .unwrap(),
    //     HashMap::from([("a".into(), ".gitignore".into())]),
    // );
    // theme
    //     .build(
    //         &PathBuf::from("temp.zip"),
    //         &BuildOptions::default().overwrite(true),
    //     )
    //     .unwrap();
}
