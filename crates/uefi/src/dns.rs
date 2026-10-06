//! DNS A queries over TCP, independent of optional firmware DNS drivers.
use crate::tcp::{Deadline, Tcp};
use alloc::{format, string::String, vec, vec::Vec};
use core::time::Duration;
use efi_agent_core::config::StaticIpv4;

pub fn resolve(
    host: &str,
    server: [u8; 4],
    port: u16,
    static_ipv4: Option<&StaticIpv4>,
    poll: &mut dyn FnMut() -> bool,
) -> Result<[u8; 4], String> {
    let parts: Vec<_> = host.split('.').collect();
    if parts.len() == 4 {
        let values: Result<Vec<u8>, _> = parts.iter().map(|part| part.parse::<u8>()).collect();
        if let Ok(values) = values {
            return Ok(values.try_into().expect("Four octets"));
        }
    }
    let mut query = vec![0x73, 0x19, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in parts {
        query.push(label.len() as u8);
        query.extend_from_slice(label.as_bytes());
    }
    query.extend_from_slice(&[0, 0, 1, 0, 1]);
    let mut tcp = Tcp::connect(server, port, static_ipv4, poll).map_err(|error| {
        format!(
            "DNS TCP connection to {}.{}.{}.{}:{} failed: {error}",
            server[0], server[1], server[2], server[3], port,
        )
    })?;
    let mut frame = Vec::new();
    frame.extend_from_slice(&(query.len() as u16).to_be_bytes());
    frame.extend(query);
    tcp.send(&frame, poll)
        .map_err(|error| format!("DNS query send failed: {error}"))?;
    let deadline = Deadline::new(Duration::from_secs(10))?;
    let mut length = [0; 2];
    read_exact(&mut tcp, &mut length, &deadline, poll)
        .map_err(|error| format!("DNS response length read failed: {error}"))?;
    let mut response = vec![0; u16::from_be_bytes(length) as usize];
    read_exact(&mut tcp, &mut response, &deadline, poll)
        .map_err(|error| format!("DNS response body read failed: {error}"))?;
    answer(&response)
}
fn read_exact(
    tcp: &mut Tcp,
    mut bytes: &mut [u8],
    deadline: &Deadline,
    poll: &mut dyn FnMut() -> bool,
) -> Result<(), String> {
    while !bytes.is_empty() {
        let count = tcp.read_some(bytes, deadline, poll)?;
        if count == 0 {
            return Err("Truncated DNS response".into());
        }
        bytes = &mut bytes[count..];
    }
    Ok(())
}
fn answer(response: &[u8]) -> Result<[u8; 4], String> {
    if response.len() < 12
        || response[..2] != [0x73, 0x19]
        || response[2] & 0x80 == 0
        || response[2] & 2 != 0
        || response[3] & 15 != 0
    {
        return Err("Invalid or unsuccessful DNS response".into());
    }
    let questions = u16::from_be_bytes([response[4], response[5]]);
    let answers = u16::from_be_bytes([response[6], response[7]]);
    let mut offset = 12;
    for _ in 0..questions {
        skip_name(response, &mut offset)?;
        offset += 4;
        if offset > response.len() {
            return Err("Truncated DNS question".into());
        }
    }
    for _ in 0..answers {
        skip_name(response, &mut offset)?;
        let header = response
            .get(offset..offset + 10)
            .ok_or("Truncated DNS answer")?;
        let size = u16::from_be_bytes([header[8], header[9]]) as usize;
        let kind = u16::from_be_bytes([header[0], header[1]]);
        let class = u16::from_be_bytes([header[2], header[3]]);
        offset += 10;
        let data = response
            .get(offset..offset + size)
            .ok_or("Truncated DNS data")?;
        if kind == 1 && class == 1 && size == 4 {
            return Ok(data.try_into().expect("Four bytes"));
        }
        offset += size;
    }
    Err("DNS response has no IPv4 address".into())
}
fn skip_name(bytes: &[u8], offset: &mut usize) -> Result<(), String> {
    loop {
        let size = *bytes.get(*offset).ok_or("Truncated DNS name")?;
        *offset += 1;
        if size == 0 {
            return Ok(());
        }
        if size & 0xc0 == 0xc0 {
            if *offset >= bytes.len() {
                return Err("Truncated DNS pointer".into());
            }
            *offset += 1;
            return Ok(());
        }
        if size > 63 {
            return Err("Invalid DNS label".into());
        }
        *offset += size as usize;
        if *offset > bytes.len() {
            return Err("Truncated DNS label".into());
        }
    }
}
