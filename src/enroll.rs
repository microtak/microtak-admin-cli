//! Device enrollment: generate a keypair + CSR, submit it to the plain
//! (unauthenticated by design) enrollment endpoint, save the result.

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

    let client = reqwest::Client::new();
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
    let cert_pem = body["signedCert"]
        .as_str()
        .context("enrollment response had no 'signedCert' field")?;

    std::fs::write(&args.out_cert, cert_pem)
        .with_context(|| format!("writing certificate to {}", args.out_cert))?;
    std::fs::write(&args.out_key, key.serialize_pem())
        .with_context(|| format!("writing key to {}", args.out_key))?;

    println!(
        "Enrolled '{}'. Certificate: {}, key: {}",
        args.cn, args.out_cert, args.out_key
    );
    Ok(())
}

/// Just enough percent-encoding for a token's own character set (hex
/// digits) -- not a general-purpose URL encoder.
fn urlencoding_minimal(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}
