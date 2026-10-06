//! Optional firmware capabilities. Detection does not grant a cryptographic trust claim.
use alloc::{format, string::String};
use uefi::{boot, proto::network::ip4config2::Ip4Config2};

#[derive(Debug)]
#[repr(transparent)]
#[uefi::proto::unsafe_protocol(uefi_raw::protocol::shell::ShellProtocol::GUID)]
struct Shell(uefi_raw::protocol::shell::ShellProtocol);

pub fn detect(startup: &str) -> String {
    let tcp4 = crate::tcp::interfaces();
    let ip4 = match boot::find_handles::<Ip4Config2>() {
        Ok(handles) => Ok(handles.len()),
        Err(error) if error.status() == uefi::Status::NOT_FOUND => Ok(0),
        Err(error) => Err(error),
    };
    let network = match (tcp4, ip4) {
        (Ok(tcp), Ok(ip)) => format!("TCP4 interfaces: {tcp}; IPv4 configuration interfaces: {ip}"),
        (Ok(tcp), Err(_)) => format!("TCP4 interfaces: {tcp}; IPv4 configuration: unavailable"),
        _ => String::from("TCP4 interface probe failed; local file tools remain available"),
    };
    let shell = match boot::find_handles::<Shell>() {
        Ok(handles) if !handles.is_empty() => {
            "UEFI Shell: protocol available; command tools pending"
        }
        Ok(_) => "UEFI Shell: unavailable (direct application boot)",
        Err(error) if error.status() == uefi::Status::NOT_FOUND => {
            "UEFI Shell: unavailable (direct application boot)"
        }
        Err(_) => "UEFI Shell: protocol probe failed",
    };
    let binding =
        crate::drivers::protocol_count(&uefi_raw::protocol::driver::DriverBindingProtocol::GUID);
    let snp = crate::drivers::protocol_count(
        &uefi_raw::protocol::network::snp::SimpleNetworkProtocol::GUID,
    );
    let mnp = crate::drivers::protocol_count(&uefi::guid!("f36ff770-a7e1-42cf-9ed2-56f0f271f44c"));
    let ip = crate::drivers::protocol_count(&uefi::guid!("c51711e7-b4bf-404a-bfb8-0a048ef1ffe4"));
    format!(
        "{network}\n{}\n{shell}\nDriver bindings: {binding}; SNP interfaces: {snp}\nMNP service bindings: {mnp}; IP4 service bindings: {ip}\n{startup}",
        probe_rng()
    )
}

fn probe_rng() -> String {
    let mut sample = [0; 32];
    let result = crate::random::fill(&mut sample);
    sample.fill(0);
    match result {
        Ok(()) => "Cryptographic RNG: RDRAND-seeded ChaCha20 ready; CPU trust required".into(),
        Err(error) => format!("Cryptographic RNG: {error:?}; TLS randomness unavailable"),
    }
}
