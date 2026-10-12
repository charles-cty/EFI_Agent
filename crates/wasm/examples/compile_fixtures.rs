use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let source = PathBuf::from(args.next().ok_or("Expected fixture directory")?);
    let output = PathBuf::from(args.next().ok_or("Expected output directory")?);
    if args.next().is_some() {
        return Err("Too many arguments".into());
    }
    fs::create_dir_all(&output)?;
    for entry in fs::read_dir(source)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "wat") {
            let bytes = wat::parse_file(&path)?;
            fs::write(
                output
                    .join(path.file_name().ok_or("Missing filename")?)
                    .with_extension("wasm"),
                bytes,
            )?;
        }
    }
    Ok(())
}
