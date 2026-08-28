use std::{env, fs, path::Path};

use ti4_review::{canonical_example, render_html, validate_bytes};

fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    let result = match args.as_slice() {
        [command, input] if command == "validate" => validate(input).map(|_| println!("valid: {input}")),
        [command, input, output] if command == "render" => render(input, output),
        [command, output] if command == "example" => example(output),
        _ => Err("usage: ti4-review validate <bundle.ti4review.json> | render <bundle.ti4review.json> <viewer.html> | example <bundle.ti4review.json>".to_owned()),
    };
    if let Err(error) = result {
        eprintln!("ti4-review: {error}");
        std::process::exit(2);
    }
}

fn validate(path: &str) -> Result<ti4_review::ReviewBundle, String> {
    validate_bytes(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}
fn render(input: &str, output: &str) -> Result<(), String> {
    let bundle = validate(input)?;
    fs::write(
        output,
        render_html(&bundle).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}
fn example(output: &str) -> Result<(), String> {
    if Path::new(output)
        .extension()
        .and_then(|value| value.to_str())
        != Some("json")
    {
        return Err("example output must be a .json bundle path".to_owned());
    }
    fs::write(
        output,
        canonical_example().map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}
