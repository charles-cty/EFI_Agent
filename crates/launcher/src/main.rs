mod boot;
mod clipboard;
mod pack;
mod paste;
mod selection;
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
        Some("package") => boot::package(args.collect()),
        _ => {
            println!(
                "EFI Agent\n  vm <qemu> <OVMF_CODE.fd> <OVMF_VARS.fd> <EFI-file-or-ESP-directory-or-image> <workspace> [--memory-mib <MiB>]\n  pack <ESP-directory> <new-disk.img>\n  package <application.efi> <output-directory>"
            );
            Ok(())
        }
    }
}
