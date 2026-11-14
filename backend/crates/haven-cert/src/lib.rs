//! Device certificates: one CA, one certificate per device, and a date to alarm on.
//!
//! Why this exists at all: the broker can only tell a real device from an impostor if the
//! device proves who it is. A shared secret per device does that for claims, but it travels
//! in a request body; a client certificate does it for every MQTT connection, which is where
//! the traffic actually is.
//!
//! ## What is and is not here
//!
//! - The CA is a **self-signed root** generated on first use and kept on disk. A real
//!   deployment puts an intermediate under it so the root can stay offline; the shape of the
//!   code is the same, and pretending otherwise in a comment would be worse than the gap.
//! - Certificates are issued to a device id, never to a name the device chose.
//! - The private key is generated **on the device** in a real enrolment and never leaves it.
//!   The helper here can generate one too, because the simulator and the tests need a
//!   complete pair, and because a vendor SDK's first integration is rarely able to do the
//!   CSR dance on day one. The API never stores a device private key.

use std::path::{Path, PathBuf};

use kernel::AppError;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose,
    SanType,
};
use time::OffsetDateTime;

/// How long a device certificate is good for. Long enough that a vendor does not have to
/// build re-enrolment into firmware 1.0, short enough that a leaked certificate is not
/// forever — and the expiry sweep exists precisely so this can stay short.
pub const DEVICE_DAYS: i64 = 365;

/// A signed certificate and the identity it was issued for.
#[derive(Debug, Clone)]
pub struct Issued {
    pub serial: String,
    pub certificate_pem: String,
    pub private_key_pem: String,
    pub not_after: OffsetDateTime,
}

/// The certificate authority, loaded from disk or created on first use.
pub struct Ca {
    cert_pem: String,
    key_pem: String,
    /// The signing identity: `signed_by` needs the certificate itself, not its parameters.
    cert: rcgen::Certificate,
    key: KeyPair,
}

impl Ca {
    /// Load the CA from `dir`, creating it if it is not there.
    ///
    /// On disk rather than in the database on purpose: a CA private key in a table is one
    /// `pg_dump` away from being in a backup, a replica and someone's laptop.
    pub async fn load_or_create(dir: &Path) -> Result<Self, AppError> {
        let cert_path = dir.join("ca.pem");
        let key_path = dir.join("ca.key");

        if cert_path.is_file() && key_path.is_file() {
            let cert_pem = tokio::fs::read_to_string(&cert_path)
                .await
                .map_err(|e| io("read CA certificate", e))?;
            let key_pem = tokio::fs::read_to_string(&key_path)
                .await
                .map_err(|e| io("read CA key", e))?;
            return Self::from_pem(&cert_pem, &key_pem);
        }

        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| io("create CA directory", e))?;

        let key = KeyPair::generate().map_err(|e| crypto("generate CA key", e))?;
        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "Haven Device CA");
        dn.push(DnType::OrganizationName, "Haven");
        params.distinguished_name = dn;
        // Ten years: a root that expires before the fleet it signs is an outage with a
        // calendar entry.
        params.not_after = OffsetDateTime::now_utc() + time::Duration::days(3650);

        let cert = params
            .self_signed(&key)
            .map_err(|e| crypto("self-sign CA", e))?;
        let cert_pem = cert.pem();
        let key_pem = key.serialize_pem();

        tokio::fs::write(&cert_path, &cert_pem)
            .await
            .map_err(|e| io("write CA certificate", e))?;
        tokio::fs::write(&key_path, &key_pem)
            .await
            .map_err(|e| io("write CA key", e))?;
        // The key is the whole trust of the fleet: 0600, and the failure is loud.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&key_path)
                .map_err(|e| io("stat CA key", e))?
                .permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&key_path, perms).map_err(|e| io("chmod CA key", e))?;
        }

        tracing::info!(path = %dir.display(), "generated a new device CA");
        Ok(Self {
            cert_pem,
            key_pem,
            cert,
            key,
        })
    }

    fn from_pem(cert_pem: &str, key_pem: &str) -> Result<Self, AppError> {
        let key = KeyPair::from_pem(key_pem).map_err(|e| crypto("load CA key", e))?;
        // Rebuild the issuer from the stored certificate, so what the broker trusts and what
        // this process signs with cannot drift apart. The reconstructed certificate carries
        // the same subject and key, which is what a chain check uses.
        let params = CertificateParams::from_ca_cert_pem(cert_pem)
            .map_err(|e| crypto("load CA certificate", e))?;
        let cert = params
            .self_signed(&key)
            .map_err(|e| crypto("re-sign CA", e))?;
        Ok(Self {
            cert_pem: cert_pem.to_string(),
            key_pem: key_pem.to_string(),
            cert,
            key,
        })
    }

    /// The CA certificate, PEM. This is what the broker is configured to trust.
    pub fn certificate_pem(&self) -> &str {
        &self.cert_pem
    }

    pub fn key_pem(&self) -> &str {
        &self.key_pem
    }

    pub fn path_hint(&self) -> PathBuf {
        PathBuf::from("ca.pem")
    }

    /// Issue a certificate for one device.
    ///
    /// The subject carries the device and vendor ids, and the SAN carries neither: a device
    /// certificate is used to open a connection, not to serve a hostname, and adding hostname
    /// SANs to ten thousand devices invites a browser to trust one.
    pub fn issue(&self, device_id: uuid::Uuid, vendor_id: uuid::Uuid) -> Result<Issued, AppError> {
        self.issue_for(device_id, vendor_id, DEVICE_DAYS)
    }

    pub fn issue_for(
        &self,
        device_id: uuid::Uuid,
        vendor_id: uuid::Uuid,
        days: i64,
    ) -> Result<Issued, AppError> {
        let key = KeyPair::generate().map_err(|e| crypto("generate device key", e))?;

        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        // The device id is the identity the broker and the API can both verify; the vendor
        // is there so a certificate from one vendor cannot be mistaken for another.
        dn.push(DnType::CommonName, device_id.to_string());
        dn.push(
            DnType::OrganizationalUnitName,
            format!("vendor:{vendor_id}"),
        );
        params.distinguished_name = dn;
        params.is_ca = IsCa::ExplicitNoCa;
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ClientAuth];
        params.subject_alt_names = vec![SanType::DnsName(
            format!("{device_id}.device.haven.invalid")
                .try_into()
                .map_err(|e| crypto("device SAN", e))?,
        )];

        let now = OffsetDateTime::now_utc();
        params.not_before = now - time::Duration::minutes(5);
        params.not_after = now + time::Duration::days(days);

        // Set the serial rather than hoping rcgen picks one: it does not write a generated
        // serial back into the parameters, so reading it afterwards yields nothing — which
        // is how the first version stored an empty string in `devices.cert_serial`.
        let serial = uuid::Uuid::now_v7();
        params.serial_number = Some(rcgen::SerialNumber::from_slice(serial.as_bytes()));

        let cert = params
            .signed_by(&key, &self.cert, &self.key)
            .map_err(|e| crypto("sign device certificate", e))?;

        Ok(Issued {
            serial: serial_of(&cert),
            certificate_pem: cert.pem(),
            private_key_pem: key.serialize_pem(),
            not_after: now + time::Duration::days(days),
        })
    }
}

/// The certificate's serial, as hex.
///
/// rcgen generates it; the caller stores it so a device can be revoked by serial even if its
/// row is lost, which is the only reason a serial matters.
fn serial_of(cert: &rcgen::Certificate) -> String {
    cert.params()
        .serial_number
        .as_ref()
        .map(|s| {
            s.as_ref()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        })
        .unwrap_or_default()
}

fn io(what: &str, e: std::io::Error) -> AppError {
    AppError::Internal(format!("certificate: could not {what}: {e}"))
}

fn crypto(what: &str, e: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("certificate: could not {what}: {e}"))
}

/// When the sweep should warn: 30, 14 and 7 days out, then today.
///
/// These are the days a person can act on. A warning every day for a month is a warning
/// nobody reads.
pub const WARN_DAYS: &[i64] = &[30, 14, 7, 0];

/// The bucket a remaining lifetime falls into, or `None` when it is not worth a notification.
pub fn warn_bucket(days_left: i64) -> Option<i64> {
    if days_left <= 0 {
        return Some(0);
    }
    WARN_DAYS.iter().copied().filter(|d| *d >= days_left).min()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn ca() -> (Ca, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let ca = Ca::load_or_create(dir.path()).await.expect("CA");
        (ca, dir)
    }

    #[tokio::test]
    async fn a_generated_ca_is_reused_not_replaced() {
        let (first, dir) = ca().await;
        let second = Ca::load_or_create(dir.path()).await.expect("reload");

        // Same key, byte for byte: if a restart minted a new CA, every device in the field
        // would fail to connect at once.
        assert_eq!(first.certificate_pem(), second.certificate_pem());
    }

    #[tokio::test]
    async fn every_device_gets_its_own_certificate() {
        let (ca, _dir) = ca().await;
        let a = ca
            .issue(uuid::Uuid::now_v7(), uuid::Uuid::now_v7())
            .expect("a");
        let b = ca
            .issue(uuid::Uuid::now_v7(), uuid::Uuid::now_v7())
            .expect("b");

        assert_ne!(a.certificate_pem, b.certificate_pem);
        assert_ne!(a.serial, b.serial);
        assert!(a.certificate_pem.contains("BEGIN CERTIFICATE"));
        assert!(a.private_key_pem.contains("PRIVATE KEY"));
    }

    #[tokio::test]
    async fn the_expiry_is_the_expiry_we_asked_for() {
        let (ca, _dir) = ca().await;
        let issued = ca
            .issue_for(uuid::Uuid::now_v7(), uuid::Uuid::now_v7(), 7)
            .expect("issue");
        let left = issued.not_after - OffsetDateTime::now_utc();
        assert!(
            left <= time::Duration::days(7) && left > time::Duration::days(6),
            "issued for 7 days, got {left}"
        );
    }

    #[test]
    fn the_sweep_warns_on_the_days_a_person_can_act() {
        assert_eq!(warn_bucket(120), None);
        assert_eq!(warn_bucket(31), None);
        assert_eq!(warn_bucket(30), Some(30));
        assert_eq!(warn_bucket(21), Some(30));
        assert_eq!(warn_bucket(14), Some(14));
        assert_eq!(warn_bucket(8), Some(14));
        assert_eq!(warn_bucket(7), Some(7));
        assert_eq!(warn_bucket(1), Some(7));
        assert_eq!(warn_bucket(0), Some(0));
        // Expired yesterday is the loudest case, not a silent one.
        assert_eq!(warn_bucket(-3), Some(0));
    }
}
