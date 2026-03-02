use dotenvor::load;

#[load(required = false, required = true)]
fn invalid() -> Result<(), dotenvor::Error> {
    Ok(())
}

fn main() {}
