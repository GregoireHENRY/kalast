use std::path::Path;

use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};

use kalast::{config::Config, util::*, Result, Scenario};

fn main() -> Result<()> {
    println!(
        "kalast<{}> (built on {} with rustc<{}>)",
        version(),
        DATETIME,
        RUSTC_VERSION
    );

    // Config file must be named cfg.toml inside cfg/ folder next to executable calling path.
    let mut builder = Figment::from(Serialized::defaults(Config::default()));

    let preferences = Path::new("preferences.toml");
    if preferences.exists() {
        builder = builder.merge(Toml::file(preferences));
    } else {
        println!("Preferences not found, using defaults.")
    }

    let path = Path::new("cfg/cfg.toml");
    if path.exists() {
        builder = builder.merge(Toml::file(path));
    } else {
        panic!("Config not found.")
    }

    let mut config: Config = builder.extract().unwrap();

    // In case it's a restart config.
    config = config.maybe_restarting();

    let mut sc = Scenario::new(config)?;

    sc.iterations()?;

    Ok(())
}
