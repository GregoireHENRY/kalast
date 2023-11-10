use kalast::prelude::*;

fn main() -> Result<()> {
    let path = Path::new(file!()).parent().unwrap();
    let sc = Scenario::new(path)?;

    let mut sc = sc.select_routines(simu::routines_thermal_default());

    sc.iterations()?;

    Ok(())
}