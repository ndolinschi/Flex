//! AWS Signature Version 4 signing for Bedrock control-plane and runtime calls.
//!
//! Self-contained: HMAC-SHA256 is built on `sha2` (no extra dependency), and
//! the timestamp is derived from `SystemTime` without a calendar crate. The
//! implementation is checked against AWS's canonical worked example (see
//! `matches_aws_documented_example`).

use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

/// AWS credentials for SigV4.
#[derive(Debug, Clone)]
pub(crate) struct Sigv4Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

/// Compute the headers to attach to a request so it is SigV4-signed: `host`,
/// `x-amz-date`, `x-amz-content-sha256`, the optional `content-type` and
/// `x-amz-security-token`, and `Authorization`. The caller must set exactly
/// these headers on the outgoing request (the signature covers them).
#[allow(clippy::too_many_arguments)]
pub(crate) fn signed_headers(
    creds: &Sigv4Credentials,
    region: &str,
    service: &str,
    method: &str,
    host: &str,
    canonical_uri: &str,
    canonical_query: &str,
    content_type: Option<&str>,
    payload: &[u8],
    amz_datetime: &str,
    amz_date: &str,
) -> Vec<(String, String)> {
    let payload_hash = hex_lower(&sha256(payload));

    // The signed header set. For non-S3 services (IAM, Bedrock, …) the AWS SDKs
    // sign `host`, `x-amz-date`, and `content-type`, and send
    // `x-amz-content-sha256` UNSIGNED — so it is appended after signing below.
    let mut headers: Vec<(String, String)> = vec![
        ("host".to_owned(), host.to_owned()),
        ("x-amz-date".to_owned(), amz_datetime.to_owned()),
    ];
    if let Some(ct) = content_type {
        headers.push(("content-type".to_owned(), ct.to_owned()));
    }
    if let Some(token) = &creds.session_token {
        headers.push(("x-amz-security-token".to_owned(), token.clone()));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));

    let canonical_headers: String = headers
        .iter()
        .map(|(name, value)| format!("{name}:{}\n", value.trim()))
        .collect();
    let signed_names: Vec<&str> = headers.iter().map(|(name, _)| name.as_str()).collect();
    let signed_headers = signed_names.join(";");

    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let credential_scope = format!("{amz_date}/{region}/{service}/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_datetime}\n{credential_scope}\n{}",
        hex_lower(&sha256(canonical_request.as_bytes()))
    );

    let signing_key = signing_key(&creds.secret_access_key, amz_date, region, service);
    let signature = hex_lower(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
        creds.access_key_id
    );

    // Return the concrete headers the caller should set on the request: the
    // signed set, the unsigned payload-hash header, and the Authorization.
    let mut out = headers;
    out.push(("x-amz-content-sha256".to_owned(), payload_hash));
    out.push(("authorization".to_owned(), authorization));
    out
}

/// Derive `(YYYYMMDDTHHMMSSZ, YYYYMMDD)` from a wall-clock instant.
pub(crate) fn amz_timestamps(now: SystemTime) -> (String, String) {
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let sod = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    let (h, mi, s) = (sod / 3600, (sod % 3600) / 60, sod % 60);
    (
        format!("{y:04}{m:02}{d:02}T{h:02}{mi:02}{s:02}Z"),
        format!("{y:04}{m:02}{d:02}"),
    )
}

/// Days-since-epoch → (year, month, day), after Howard Hinnant's `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> [u8; 32] {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// HMAC-SHA256 (RFC 2104) over `sha2`, avoiding an extra crate.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut block_key = [0u8; BLOCK];
    if key.len() > BLOCK {
        block_key[..32].copy_from_slice(&sha256(key));
    } else {
        block_key[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for (byte, k) in ipad.iter_mut().zip(block_key.iter()) {
        *byte ^= k;
    }
    for (byte, k) in opad.iter_mut().zip(block_key.iter()) {
        *byte ^= k;
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    let mut out = [0u8; 32];
    out.copy_from_slice(&outer.finalize());
    out
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_payload_hash_is_the_known_constant() {
        assert_eq!(
            hex_lower(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hmac_rfc_test_vector() {
        // RFC 4231 test case 2: key "Jefe", data "what do ya want for nothing?".
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex_lower(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn civil_from_days_epoch_and_known_date() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2015-08-30 is 16677 days after the epoch.
        assert_eq!(civil_from_days(16_677), (2015, 8, 30));
    }

    #[test]
    fn matches_aws_documented_example() {
        // AWS "Signature Version 4" canonical example: a GET to the IAM API.
        // https://docs.aws.amazon.com/.../sigv4-create-canonical-request.html
        let creds = Sigv4Credentials {
            access_key_id: "AKIDEXAMPLE".to_owned(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned(),
            session_token: None,
        };
        let headers = signed_headers(
            &creds,
            "us-east-1",
            "iam",
            "GET",
            "iam.amazonaws.com",
            "/",
            "Action=ListUsers&Version=2010-05-08",
            Some("application/x-www-form-urlencoded; charset=utf-8"),
            b"",
            "20150830T123600Z",
            "20150830",
        );
        let authorization = headers
            .iter()
            .find(|(name, _)| name == "authorization")
            .map(|(_, value)| value.clone())
            .expect("authorization header present");
        assert_eq!(
            authorization,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request, \
             SignedHeaders=content-type;host;x-amz-date, \
             Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        );
    }

    #[test]
    fn session_token_is_signed_when_present() {
        let creds = Sigv4Credentials {
            access_key_id: "AKID".to_owned(),
            secret_access_key: "secret".to_owned(),
            session_token: Some("tok".to_owned()),
        };
        let headers = signed_headers(
            &creds,
            "us-east-1",
            "bedrock",
            "GET",
            "bedrock.us-east-1.amazonaws.com",
            "/foundation-models",
            "",
            None,
            b"",
            "20240101T000000Z",
            "20240101",
        );
        assert!(
            headers
                .iter()
                .any(|(n, v)| n == "x-amz-security-token" && v == "tok")
        );
        let auth = headers
            .iter()
            .find(|(n, _)| n == "authorization")
            .map(|(_, v)| v.clone());
        assert!(auth.unwrap_or_default().contains("x-amz-security-token"));
    }
}
