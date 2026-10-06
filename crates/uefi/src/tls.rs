//! Direct TLS transport with certificate, hostname, clock, and RNG validation.
use crate::tcp::{Deadline, Tcp};
use alloc::string::ToString;
use alloc::{format, string::String, sync::Arc, vec, vec::Vec};
use core::time::Duration;
use efi_agent_core::config::StaticIpv4;
use rustls::{
    ClientConfig, RootCertStore,
    client::UnbufferedClientConnection,
    pki_types::{CertificateDer, ServerName, UnixTime},
    unbuffered::ConnectionState,
};

fn random(bytes: &mut [u8]) -> Result<(), getrandom::Error> {
    let failure = || {
        getrandom::Error::from(
            core::num::NonZeroU32::new(getrandom::Error::CUSTOM_START).expect("Nonzero"),
        )
    };
    crate::random::fill(bytes).map_err(|_| failure())
}
getrandom::register_custom_getrandom!(random);

#[derive(Debug)]
struct FirmwareTime;
impl rustls::time_provider::TimeProvider for FirmwareTime {
    fn current_time(&self) -> Option<UnixTime> {
        let time = uefi::runtime::get_time().ok()?;
        // UEFI's timezone is minutes east of UTC. An unspecified timezone
        // uses the firmware convention of a UTC hardware clock.
        let seconds = efi_agent_core::clock::unix_seconds(
            time.year(),
            time.month(),
            time.day(),
            time.hour(),
            time.minute(),
            time.second(),
            time.time_zone().unwrap_or(0),
        )?;
        Some(UnixTime::since_unix_epoch(Duration::from_secs(seconds)))
    }
}

pub struct Socket<'a> {
    tcp: Tcp,
    tls: Option<UnbufferedClientConnection>,
    incoming: Vec<u8>,
    outgoing: Vec<u8>,
    plaintext: Vec<u8>,
    offset: usize,
    deadline: Deadline,
    poll: &'a mut dyn FnMut() -> bool,
    closed: bool,
}
impl<'a> Socket<'a> {
    pub fn connect(
        url: &efi_agent_core::http::Url,
        address: [u8; 4],
        ca: Option<Vec<u8>>,
        static_ipv4: Option<&StaticIpv4>,
        poll: &'a mut dyn FnMut() -> bool,
    ) -> Result<Self, String> {
        let tls = if url.tls {
            let mut probe = [0; 1];
            crate::random::fill(&mut probe)
                .map_err(|error| format!("TLS random generator: {error:?}"))?;
            let mut roots = RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            if let Some(ca) = ca {
                roots
                    .add(CertificateDer::from(ca))
                    .map_err(|e| format!("Invalid CA certificate: {e}"))?;
            }
            let config = ClientConfig::builder_with_details(
                Arc::new(rustls_rustcrypto::provider()),
                Arc::new(FirmwareTime),
            )
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_root_certificates(roots)
            .with_no_client_auth();
            let name =
                ServerName::try_from(url.host.clone()).map_err(|_| "Invalid TLS server name")?;
            Some(
                UnbufferedClientConnection::new(Arc::new(config), name)
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let tcp = Tcp::connect(address, url.port, static_ipv4, poll)?;
        Ok(Self {
            tcp,
            tls,
            incoming: Vec::new(),
            outgoing: vec![0; 65536],
            plaintext: Vec::new(),
            offset: 0,
            deadline: Deadline::new(Duration::from_secs(120))?,
            poll,
            closed: false,
        })
    }
    fn pump(&mut self, data: Option<&[u8]>) -> Result<(), String> {
        if (self.poll)() {
            return Err("Request cancelled".into());
        }
        if self.deadline.expired() {
            return Err("Provider request timed out".into());
        }
        let mut sending = data;
        let mut encoded = 0;
        loop {
            let tls = self.tls.as_mut().ok_or("TLS not configured")?;
            let status = tls.process_tls_records(&mut self.incoming);
            let mut discard = status.discard;
            let mut receive = false;
            let mut finished = false;
            match status.state.map_err(|e| format!("TLS: {e}"))? {
                ConnectionState::EncodeTlsData(mut state) => {
                    encoded = state
                        .encode(&mut self.outgoing)
                        .map_err(|e| format!("TLS encode: {e:?}"))?;
                }
                ConnectionState::TransmitTlsData(state) => {
                    self.tcp.send(&self.outgoing[..encoded], self.poll)?;
                    encoded = 0;
                    state.done();
                }
                ConnectionState::BlockedHandshake => receive = true,
                ConnectionState::WriteTraffic(mut state) => {
                    if let Some(bytes) = sending.take() {
                        let count = state
                            .encrypt(bytes, &mut self.outgoing)
                            .map_err(|e| format!("TLS encrypt: {e:?}"))?;
                        self.tcp.send(&self.outgoing[..count], self.poll)?;
                        finished = true;
                    } else if data.is_some() {
                        finished = true;
                    } else {
                        receive = true;
                    }
                }
                ConnectionState::ReadTraffic(mut state) => {
                    while let Some(record) = state.next_record() {
                        let record = record.map_err(|e| format!("TLS record: {e}"))?;
                        discard += record.discard;
                        self.plaintext.extend_from_slice(record.payload);
                    }
                    finished = data.is_none() && !self.plaintext.is_empty();
                }
                ConnectionState::PeerClosed | ConnectionState::Closed => {
                    self.closed = true;
                    finished = true;
                }
                _ => return Err("Unsupported TLS connection state".into()),
            }
            self.incoming.drain(..discard);
            if finished {
                return Ok(());
            }
            if receive {
                let mut bytes = [0; 16384];
                let count = self.tcp.read_some(&mut bytes, &self.deadline, self.poll)?;
                if count == 0 {
                    return Err("TLS peer closed before close_notify".into());
                }
                if self.incoming.len() + count > 128 * 1024 {
                    return Err("TLS record buffer exceeds limit".into());
                }
                self.incoming.extend_from_slice(&bytes[..count]);
            }
        }
    }
    pub fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self.tls.is_none() {
            return self.tcp.send(bytes, self.poll);
        }
        for chunk in bytes.chunks(16384) {
            self.pump(Some(chunk))?;
        }
        Ok(())
    }
}
impl efi_agent_core::model::Read for Socket<'_> {
    fn read(&mut self, output: &mut [u8]) -> Result<usize, String> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.tls.is_none() {
            return self.tcp.read_some(output, &self.deadline, self.poll);
        }
        if self.offset == self.plaintext.len() {
            self.plaintext.clear();
            self.offset = 0;
            if !self.closed {
                self.pump(None)?;
            }
        }
        let count = output.len().min(self.plaintext.len() - self.offset);
        output[..count].copy_from_slice(&self.plaintext[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}
