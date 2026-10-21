fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", learner_progress_service::routes::openapi().to_yaml()?);
    Ok(())
}
