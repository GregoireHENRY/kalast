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

    let config: Config = Figment::from(Serialized::defaults(Config::default()))
        .merge(Toml::file("preferences.toml"))
        .merge(Toml::file("cfg/cfg.toml"))
        .extract()
        .unwrap();

    let mut sc = Scenario::new(config)?;

    sc.iterations()?;

    Ok(())
}
