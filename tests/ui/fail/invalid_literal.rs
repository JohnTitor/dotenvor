use dotenvor::load;

#[load(required = "yes")]
fn invalid() -> Result<(), dotenvor::Error> {
    Ok(())
}

fn main() {}
