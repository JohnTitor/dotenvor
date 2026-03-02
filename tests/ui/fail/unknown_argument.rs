use dotenvor::load;

#[load(unknown = true)]
fn invalid() -> Result<(), dotenvor::Error> {
    Ok(())
}

fn main() {}
