"""Independent short-lived certificates for actual guest TLS integration."""
from datetime import datetime, timedelta, timezone
import ipaddress
import ssl

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.x509.oid import NameOID


# curl 8.22.0 with wolfSSL keeps handshake, CA and peer errors distinct.
CURL_TLS_EXIT_CODES = {
    "tls12": 0,
    "tls13": 0,
    "untrusted": 35,
    "no_signer": 77,
    "hostname": 60,
    "expired": 60,
}


def tls_context(directory, failure=None):
    now = datetime.now(timezone.utc)
    key = ec.generate_private_key(ec.SECP256R1())
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "WASM test CA")])

    def authority(authority_key, name=subject):
        return (x509.CertificateBuilder().subject_name(name).issuer_name(name)
                .public_key(authority_key.public_key()).serial_number(x509.random_serial_number())
                .not_valid_before(now - timedelta(days=2)).not_valid_after(now + timedelta(days=2))
                .add_extension(x509.BasicConstraints(ca=True, path_length=0), critical=True)
                .sign(authority_key, hashes.SHA256()))

    ca = authority(key)
    trust = ca
    if failure in ["untrusted", "no_signer"]:
        name = subject if failure == "untrusted" else x509.Name([
            x509.NameAttribute(NameOID.COMMON_NAME, "Unrelated test CA")])
        trust = authority(ec.generate_private_key(ec.SECP256R1()), name)
    server_key = ec.generate_private_key(ec.SECP256R1())
    addresses = ["192.0.2.1"] if failure == "hostname" else ["127.0.0.1", "10.0.2.2"]
    certificate = (x509.CertificateBuilder().subject_name(x509.Name([
        x509.NameAttribute(NameOID.COMMON_NAME, "WASM HTTP test")]))
        .issuer_name(subject).public_key(server_key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - timedelta(days=2))
        .not_valid_after(now - timedelta(days=1) if failure == "expired" else now + timedelta(days=1))
        .add_extension(x509.SubjectAlternativeName([
            x509.IPAddress(ipaddress.ip_address(address)) for address in addresses]), critical=False)
        .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True)
        .sign(key, hashes.SHA256()))
    (directory / "curl-ca.pem").write_bytes(trust.public_bytes(serialization.Encoding.PEM))
    certificate_path = directory / "curl-cert.pem"
    key_path = directory / "curl-key.pem"
    certificate_path.write_bytes(certificate.public_bytes(serialization.Encoding.PEM))
    key_path.write_bytes(server_key.private_bytes(serialization.Encoding.PEM,
                                                 serialization.PrivateFormat.PKCS8,
                                                 serialization.NoEncryption()))
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.load_cert_chain(certificate_path, key_path)
    return context
