//! Pairing tickets (app side) and claim (device side).

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::post;
use domain::pairing::{ClaimRequest, TICKET_TTL_SECS};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::Auth;
use crate::error::{ApiError, ApiResult};
use crate::limits::{self, Peer};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/homes/{id}/pairing-tickets", post(mint))
        .route("/provision/claim", post(claim))
        // Certificate issuance: the device proves who it is with the secret it was given at
        // claim time, and walks away with a certificate for every MQTT connection after.
        .route("/provision/certificate", post(certificate))
}

#[derive(Serialize)]
struct TicketBody {
    ticket: String,
    expires_in: i64,
}

async fn mint(
    State(state): State<AppState>,
    auth: Auth,
    Path(home_id): Path<Uuid>,
) -> ApiResult<(StatusCode, Json<TicketBody>)> {
    // Adding hardware is an admin action on the home.
    let home = state
        .homes
        .require(home_id, auth.user.id)
        .await
        .map_err(ApiError)?;
    if !home.role.can_manage() {
        return Err(ApiError(kernel::AppError::forbidden(
            "only an admin can add a device to this home",
        )));
    }

    let ticket = state
        .pairing
        .mint(home_id, auth.user.id, kernel::new_id())
        .await
        .map_err(ApiError)?;

    Ok((
        StatusCode::CREATED,
        Json(TicketBody {
            ticket,
            expires_in: TICKET_TTL_SECS,
        }),
    ))
}

#[derive(Deserialize)]
struct ClaimBody {
    pk: String,
    sn: String,
    secret: String,
    ticket: String,
    #[serde(default)]
    fw: Option<String>,
}

#[derive(Serialize)]
struct ClaimResponse {
    device_id: Uuid,
    vendor_id: Uuid,
    home_id: Uuid,
    name: String,
    points: usize,
    mqtt: MqttConfig,
}

#[derive(Serialize)]
struct MqttConfig {
    host: String,
    port: u16,
    client_id: String,
    keepalive: u16,
}

/// Device-facing: no bearer token, the ticket and the device secret are the
/// credentials. Deliberately not rate-limited by user, so the limiter is per
/// serial in the claim path itself.
async fn claim(
    State(state): State<AppState>,
    peer: Peer,
    Json(body): Json<ClaimBody>,
) -> ApiResult<(StatusCode, Json<ClaimResponse>)> {
    // Keyed by source and serial: a misconfigured device retries in a loop, and that
    // loop must not become a denial of service on somebody's home.
    let key = format!("rl:claim:{}:{}", peer.key(), body.sn);
    limits::check(&state, &key, limits::CLAIM).await?;

    let claimed = match state
        .pairing
        .claim(ClaimRequest {
            product_key: body.pk,
            serial: body.sn,
            secret: body.secret,
            ticket: body.ticket,
            fw: body.fw,
        })
        .await
    {
        Ok(claimed) => claimed,
        Err(e) => {
            limits::record(&state, &key, limits::CLAIM).await;
            return Err(ApiError(e));
        }
    };

    let (host, port) = mqtt_endpoint(&state.cfg.mqtt_url);

    Ok((
        StatusCode::CREATED,
        Json(ClaimResponse {
            device_id: claimed.device_id,
            vendor_id: claimed.vendor_id,
            home_id: claimed.home_id,
            name: claimed.name,
            points: claimed.points,
            mqtt: MqttConfig {
                host,
                port,
                client_id: claimed.device_id.to_string(),
                keepalive: 30,
            },
        }),
    ))
}

/// `mqtt://host:1883` → the address a device dials. TLS is chosen by the port
/// the deployment advertises (8883), and the certificate is issued later in the
/// claim flow.
fn mqtt_endpoint(url: &str) -> (String, u16) {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let rest = rest.trim_end_matches('/');
    match rest.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().unwrap_or(1883)),
        None => (rest.to_string(), 1883),
    }
}

#[cfg(test)]
mod tests {
    use super::mqtt_endpoint;

    #[test]
    fn mqtt_endpoint_parses() {
        assert_eq!(
            mqtt_endpoint("mqtt://localhost:1883"),
            ("localhost".to_string(), 1883)
        );
        assert_eq!(mqtt_endpoint("mqtts://h"), ("h".to_string(), 1883));
    }
}

#[derive(Deserialize)]
struct CertificateBody {
    pk: String,
    sn: String,
    secret: String,
    /// The device's own key, as a PKCS#10 request. Present: the platform signs this key and never
    /// sees a private key. Absent: the platform generates the pair, which is what a first
    /// integration that cannot do the CSR dance needs.
    #[serde(default)]
    csr_pem: Option<String>,
}

#[derive(Serialize)]
struct CertificateResponse {
    device_id: Uuid,
    /// The device's client certificate.
    certificate_pem: String,
    /// Only when the platform generated the key. A device that sends a CSR keeps its private key
    /// and this field is absent — which is why it is `Option` rather than an empty string a client
    /// might try to parse.
    #[serde(skip_serializing_if = "Option::is_none")]
    private_key_pem: Option<String>,
    /// What the broker is configured to trust, so a device does not have to ship it.
    ca_pem: String,
    serial: String,
    /// RFC 3339, like every other timestamp the API emits — the default `time` serde
    /// representation is a nine-element array, which is not a date anyone can read.
    #[serde(with = "time::serde::rfc3339")]
    not_after: time::OffsetDateTime,
}

/// Device-facing: no bearer token. The device secret is the credential, and it is checked
/// against the hash stored at claim time — the same check, the same failure message.
async fn certificate(
    State(state): State<AppState>,
    peer: Peer,
    Json(body): Json<CertificateBody>,
) -> ApiResult<(StatusCode, Json<CertificateResponse>)> {
    // Issuance is expensive (key generation + signing) and a misconfigured device retries in
    // a loop, so it gets the same per-serial limiter as claiming.
    let key = format!("rl:cert:{}:{}", peer.key(), body.sn);
    limits::check(&state, &key, limits::CLAIM).await?;

    let device = match state
        .pairing
        .authenticate(&body.pk, &body.sn, &body.secret)
        .await
    {
        Ok(device) => device,
        Err(e) => {
            limits::record(&state, &key, limits::CLAIM).await;
            return Err(ApiError(e));
        }
    };

    // A CSR is a few kilobytes at most; anything larger is not one, and refusing it before the
    // parser sees it keeps a malformed request cheap.
    if body
        .csr_pem
        .as_ref()
        .is_some_and(|csr| csr.len() > 8 * 1024)
    {
        return Err(ApiError(kernel::AppError::validation(
            "certificate: the CSR is larger than 8 KiB",
        )));
    }

    let ca = state.ca().await.map_err(ApiError)?;
    let issued = match body.csr_pem.as_deref() {
        Some(csr) => ca.issue_from_csr(
            csr,
            device.device_id,
            device.vendor_id,
            haven_cert::DEVICE_DAYS,
        ),
        None => ca.issue(device.device_id, device.vendor_id),
    }
    .map_err(ApiError)?;

    // Recorded before it is returned: a certificate the platform cannot see the expiry of is
    // a device that stops connecting with no warning anywhere.
    state
        .pairing
        .record_certificate(device.device_id, &issued.serial, issued.not_after)
        .await
        .map_err(ApiError)?;

    tracing::info!(
        device = %device.device_id,
        serial = %issued.serial,
        not_after = %issued.not_after,
        csr = body.csr_pem.is_some(),
        "issued a device certificate"
    );

    Ok((
        StatusCode::CREATED,
        Json(CertificateResponse {
            device_id: device.device_id,
            certificate_pem: issued.certificate_pem,
            private_key_pem: issued.private_key_pem,
            ca_pem: ca.certificate_pem().to_string(),
            serial: issued.serial,
            not_after: issued.not_after,
        }),
    ))
}
