use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};

use kalast::{config::Config, path_cfg_folder, Result, Scenario};

fn main() -> Result<()> {
    dbg!(path_cfg_folder().join("cfg.toml"));

    let config: Config = Figment::from(Serialized::defaults(Config::default()))
        .merge(Toml::file("preferences.toml"))
        .merge(Toml::file("cfg/cfg.toml"))
        .extract()
        .unwrap();

    let mut sc = Scenario::new(config)?;

    sc.iterations()?;

    Ok(())
}
