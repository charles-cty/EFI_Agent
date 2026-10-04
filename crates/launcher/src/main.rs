mod bridge;
mod model;
mod pack;
mod vm;

fn main() {
    if let Err(error) = run() {
        eprintln!("EFI Agent: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("vm") => vm::run(args.collect()),
        Some("pack") => pack::run(args.collect()),
        Some("serve") => {
            model::validate_configuration()?;
            let root = std::path::PathBuf::from(
                args.next()
                    .ok_or("Usage: efi-agent serve <workspace> [address]")?,
            )
            .canonicalize()?;
            let address = args.next().unwrap_or_else(|| "127.0.0.1:7420".into());
            let listener = std::net::TcpListener::bind(&address)?;
            eprintln!("HostBridge listening on {}", listener.local_addr()?);
            bridge::serve(listener, root)
                .join()
                .map_err(|_| "HostBridge thread failed")?;
            Ok(())
        }
        _ => {
            println!(
                "EFI Agent\n  vm <qemu> <OVMF_CODE.fd> <OVMF_VARS.fd> <ESP-directory-or-image> <workspace> [--memory-mib <MiB>]\n  serve <workspace> [address]\n  pack <ESP-directory> <new-disk.img>"
            );
            Ok(())
        }
    }
}
