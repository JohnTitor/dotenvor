use dotenvor::load;

#[load(path = "tests/fixtures/macro-basic.env")]
fn load_ok() -> Result<(), dotenvor::Error> {
    Ok(())
}

fn main() {
    let _ = unsafe { load_ok() };
}
