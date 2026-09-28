//! Device enrollment: generate a keypair + CSR, submit it to the server's
//! HTTPS enrollment endpoint (no client cert -- the device has none yet),
//! save the result. The endpoint is verified against `--ca` (the server's
//! own CA), or the system roots if it has a publicly-trusted certificate;
//! verification is never skipped.

use anyhow::{bail, Context, Result};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};

use crate::cli::EnrollArgs;

fn build_csr(common_name: &str) -> Result<(String, KeyPair)> {
    let key = KeyPair::generate().context("generating a keypair")?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, common_name);
    let mut params =
        CertificateParams::new(Vec::<String>::new()).context("building CSR params")?;
    params.distinguished_name = dn;
    let csr = params.serialize_request(&key).context("serializing CSR")?;
    Ok((csr.pem().context("PEM-encoding CSR")?, key))
}

pub async fn run(args: EnrollArgs) -> Result<()> {
    let (csr_pem, key) = build_csr(&args.cn)?;

    let mut url = format!("{}/Marti/api/tls/signClient/v2", args.enrollment_url);
    if let Some(token) = &args.token {
        url.push_str("?token=");
        url.push_str(&urlencoding_minimal(token));
    }

    let client = enrollment_client(&args)?;
    let response = client
        .post(&url)
        .header("Content-Type", "application/octet-stream")
        .body(csr_pem)
        .send()
        .await
        .context("submitting the CSR to the enrollment endpoint")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("enrollment failed ({status}): {body}");
    }

    let body: serde_json::Value = response.json().await.context("parsing enrollment response")?;
    let bare_cert = body["signedCert"]
        .as_str()
        .context("enrollment response had no 'signedCert' field")?;
    let cert_pem = wrap_pem_certificate(bare_cert);
    if let Some(out_ca) = &args.out_ca {
        let ca_pem = wrap_pem_certificate(
            body["ca0"]
                .as_str()
                .context("enrollment response had no 'ca0' field")?,
        );
        std::fs::write(out_ca, ca_pem).with_context(|| format!("writing CA certificate to {out_ca}"))?;
    }

    std::fs::write(&args.out_cert, &cert_pem)
        .with_context(|| format!("writing certificate to {}", args.out_cert))?;
    std::fs::write(&args.out_key, key.serialize_pem())
        .with_context(|| format!("writing key to {}", args.out_key))?;

    println!(
        "Enrolled '{}'. Certificate: {}, key: {}",
        args.cn, args.out_cert, args.out_key
    );
    Ok(())
}

fn require_https(url: &str) -> Result<()> {
    if !url.to_ascii_lowercase().starts_with("https://") {
        bail!(
            "the enrollment endpoint is HTTPS only -- use an https:// URL (got '{url}')"
        );
    }
    Ok(())
}

fn enrollment_client(args: &EnrollArgs) -> Result<reqwest::Client> {
    require_https(&args.enrollment_url)?;
    let mut builder = reqwest::Client::builder().https_only(true);
    if let Some(ca_path) = &args.ca {
        let ca_pem = std::fs::read(ca_path).with_context(|| format!("reading CA certificate {ca_path}"))?;
        let ca_cert = reqwest::Certificate::from_pem(&ca_pem)
            .with_context(|| format!("parsing CA certificate {ca_path}"))?;
        builder = builder.add_root_certificate(ca_cert);
    }
    if let Some(resolve) = &args.resolve {
        let (host, addr) = crate::client::parse_resolve(resolve)?;
        builder = builder.resolve(&host, addr);
    }
    builder.build().context("building the HTTPS client")
}

/// Just enough percent-encoding for a token's own character set (hex
/// digits) -- not a general-purpose URL encoder.
fn urlencoding_minimal(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

/// Re-armor a bare-base64 `signedCert` value into full PEM. Real Marti
/// wire format: the server strips PEM armor before responding (see
/// microtak-server's `marti::enrollment` module doc comment), matching
/// real TAK-Server/node-tak client behavior, which re-wraps it itself
/// before use -- so do the same here.
fn wrap_pem_certificate(bare_base64: &str) -> String {
    format!("-----BEGIN CERTIFICATE-----\n{bare_base64}\n-----END CERTIFICATE-----\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_plain_http_enrollment_urls() {
        assert!(require_https("http://microtak.example.com:8446").is_err());
        assert!(require_https("HTTP://x:8446").is_err());
        assert!(require_https("https://microtak.example.com:8446").is_ok());
    }

    #[test]
    fn wraps_a_bare_base64_cert_into_valid_pem() {
        let wrapped = wrap_pem_certificate("bm90LWEtcmVhbC1jZXJ0");
        assert!(wrapped.starts_with("-----BEGIN CERTIFICATE-----\n"));
        assert!(wrapped.ends_with("-----END CERTIFICATE-----\n"));
        assert!(wrapped.contains("bm90LWEtcmVhbC1jZXJ0"));
    }
}
