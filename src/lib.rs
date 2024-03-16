use std::{collections::HashMap, path::PathBuf};

use ini::Ini;
use theme::BuildOptions;

mod theme;

pub fn setup_logging() {
    use env_logger::Env;

    let env = Env::default().default_filter_or("reaper_theme_builder_2=warn");

    env_logger::init_from_env(env);
}

pub fn hello() {
    setup_logging();

    let theme = theme::Theme::new(
        "temp",
        "; this is rtconfig code",
        Ini::load_from_str("[config]\nhello=world").unwrap(),
        HashMap::from([("a".into(), ".gitignore".into())]),
    );
    theme
        .build(
            &PathBuf::from("temp.zip"),
            &BuildOptions::default().overwrite(true),
        )
        .unwrap();
}
