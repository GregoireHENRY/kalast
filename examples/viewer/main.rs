use kalast::prelude::*;

fn main() -> Result<()> {
    let path = Path::new(file!()).parent().unwrap();
    let mut sc = Scenario::new(path)?;

    sc.iterations()?;

    Ok(())
}