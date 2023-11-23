use kalast::{Result, Scenario};

fn main() -> Result<()> {
    let mut sc = Scenario::new()?;

    sc.iterations()?;

    Ok(())
}
