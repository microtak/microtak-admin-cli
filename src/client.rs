//! Builds an mTLS-authenticated `reqwest` client from an
//! [`crate::cli::AdminConnection`], and the small set of API calls this CLI
//! needs against MicroTAK's admin/mission-role endpoints.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use crate::cli::{AdminConnection, RoleArg};

pub fn build_client(conn: &AdminConnection) -> Result<reqwest::Client> {
    let mut identity_pem = std::fs::read(&conn.cert)
        .with_context(|| format!("reading client certificate {}", conn.cert))?;
    identity_pem.extend_from_slice(
        &std::fs::read(&conn.key).with_context(|| format!("reading client key {}", conn.key))?,
    );
    let identity = reqwest::Identity::from_pem(&identity_pem).context("parsing client cert/key as a TLS identity")?;
    let ca_pem = std::fs::read(&conn.ca).with_context(|| format!("reading CA certificate {}", conn.ca))?;
    let ca_cert = reqwest::Certificate::from_pem(&ca_pem).context("parsing CA certificate")?;

    let mut builder = reqwest::Client::builder()
        .identity(identity)
        .add_root_certificate(ca_cert);

    if let Some(resolve) = &conn.resolve {
        let (host, addr) = parse_resolve(resolve)?;
        builder = builder.resolve(&host, addr);
    }

    builder.build().context("building HTTP client")
}

/// Parse curl's `--resolve HOST:PORT:ADDRESS` syntax into what
/// `reqwest::ClientBuilder::resolve` wants (a hostname and a `SocketAddr`).
fn parse_resolve(spec: &str) -> Result<(String, SocketAddr)> {
    let mut parts = spec.splitn(3, ':');
    let (Some(host), Some(port), Some(address)) = (parts.next(), parts.next(), parts.next()) else {
        bail!("--resolve must be in the form HOST:PORT:ADDRESS, got '{spec}'");
    };
    // An IPv6 literal needs bracket notation to combine with a port at all
    // (`::1:8443` is ambiguous/invalid; `[::1]:8443` isn't) -- bracket it
    // if it looks like one and isn't already.
    let bracketed = if address.contains(':') && !address.starts_with('[') {
        format!("[{address}]")
    } else {
        address.to_string()
    };
    let socket_addr: SocketAddr = format!("{bracketed}:{port}")
        .parse()
        .with_context(|| format!("'{address}:{port}' is not a valid address:port"))?;
    Ok((host.to_string(), socket_addr))
}

#[derive(Debug, Deserialize)]
pub struct EnrollmentToken {
    pub token: String,
    pub created_at_unix: i64,
    pub expires_at_unix: Option<i64>,
    pub note: Option<String>,
    pub used: bool,
    pub used_by_common_name: Option<String>,
    pub revoked: bool,
}

#[derive(Serialize)]
struct MintTokenRequest {
    #[serde(rename = "expiresInSecs", skip_serializing_if = "Option::is_none")]
    expires_in_secs: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

pub async fn mint_token(
    client: &reqwest::Client,
    conn: &AdminConnection,
    expires_in_secs: Option<i64>,
    note: Option<String>,
) -> Result<String> {
    let response = client
        .post(format!("{}/Marti/api/admin/enrollmentTokens", conn.server))
        .json(&MintTokenRequest { expires_in_secs, note })
        .send()
        .await
        .context("calling the admin API to mint a token")?;
    let response = check_status(response).await?;
    let body: serde_json::Value = response.json().await.context("parsing mint response")?;
    body["token"]
        .as_str()
        .map(str::to_string)
        .context("mint response had no 'token' field")
}

pub async fn list_tokens(client: &reqwest::Client, conn: &AdminConnection) -> Result<Vec<EnrollmentToken>> {
    let response = client
        .get(format!("{}/Marti/api/admin/enrollmentTokens", conn.server))
        .send()
        .await
        .context("calling the admin API to list tokens")?;
    let response = check_status(response).await?;
    response.json().await.context("parsing token list response")
}

pub async fn revoke_token(client: &reqwest::Client, conn: &AdminConnection, token: &str) -> Result<()> {
    let response = client
        .delete(format!("{}/Marti/api/admin/enrollmentTokens/{token}", conn.server))
        .send()
        .await
        .context("calling the admin API to revoke a token")?;
    check_status(response).await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub created_at_unix: i64,
    pub revoked: bool,
}

#[derive(Serialize)]
struct MintUserRequest {
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
}

/// Mint a new password account. If `password` is `None`, the server
/// generates one and returns it -- this function returns whatever password
/// is now actually in effect either way, since the caller needs to display
/// it (server-generated case) or already knows it (explicit case).
pub async fn mint_user(
    client: &reqwest::Client,
    conn: &AdminConnection,
    username: &str,
    password: Option<String>,
) -> Result<String> {
    let response = client
        .post(format!("{}/Marti/api/admin/users", conn.server))
        .json(&MintUserRequest {
            username: username.to_string(),
            password,
        })
        .send()
        .await
        .context("calling the admin API to mint a user")?;
    let response = check_status(response).await?;
    let body: serde_json::Value = response.json().await.context("parsing mint response")?;
    body["password"]
        .as_str()
        .map(str::to_string)
        .context("mint response had no 'password' field")
}

pub async fn list_users(client: &reqwest::Client, conn: &AdminConnection) -> Result<Vec<UserInfo>> {
    let response = client
        .get(format!("{}/Marti/api/admin/users", conn.server))
        .send()
        .await
        .context("calling the admin API to list users")?;
    let response = check_status(response).await?;
    response.json().await.context("parsing user list response")
}

pub async fn revoke_user(client: &reqwest::Client, conn: &AdminConnection, username: &str) -> Result<()> {
    let response = client
        .delete(format!("{}/Marti/api/admin/users/{}", conn.server, urlencode(username)))
        .send()
        .await
        .context("calling the admin API to revoke a user")?;
    check_status(response).await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct Mission {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "creator_uid")]
    pub creator_uid: String,
    pub roles: std::collections::BTreeMap<String, String>,
    pub subscribers: Vec<String>,
}

pub async fn list_missions(client: &reqwest::Client, conn: &AdminConnection) -> Result<Vec<Mission>> {
    let response = client
        .get(format!("{}/Marti/api/missions", conn.server))
        .send()
        .await
        .context("listing missions")?;
    let response = check_status(response).await?;
    response.json().await.context("parsing mission list response")
}

pub async fn get_mission(client: &reqwest::Client, conn: &AdminConnection, name: &str) -> Result<Mission> {
    let response = client
        .get(format!(
            "{}/Marti/api/missions/{}",
            conn.server,
            urlencode(name)
        ))
        .send()
        .await
        .context("fetching mission")?;
    let response = check_status(response).await?;
    response.json().await.context("parsing mission response")
}

#[derive(Serialize)]
struct SetRoleRequest {
    uid: String,
    role: String,
}

pub async fn set_role(
    client: &reqwest::Client,
    conn: &AdminConnection,
    mission: &str,
    uid: &str,
    role: RoleArg,
) -> Result<()> {
    let response = client
        .put(format!(
            "{}/Marti/api/missions/{}/role",
            conn.server,
            urlencode(mission)
        ))
        .json(&SetRoleRequest {
            uid: uid.to_string(),
            role: role.to_string(),
        })
        .send()
        .await
        .context("assigning a mission role")?;
    check_status(response).await?;
    Ok(())
}

pub async fn revoke_role(client: &reqwest::Client, conn: &AdminConnection, mission: &str, uid: &str) -> Result<()> {
    let response = client
        .delete(format!(
            "{}/Marti/api/missions/{}/role/{}",
            conn.server,
            urlencode(mission),
            urlencode(uid)
        ))
        .send()
        .await
        .context("revoking a mission role")?;
    check_status(response).await?;
    Ok(())
}

/// Minimal percent-encoding for a path segment -- mission names may
/// contain spaces and other URL-reserved characters (see MicroTAK's own
/// TC-MARTI-03), so this can't just be string-concatenated in.
fn urlencode(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Turn a non-2xx response into a real error carrying the server's own
/// `{"error": "..."}` message when present, rather than a bare status code.
async fn check_status(response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_string))
        .unwrap_or(body);
    bail!("server returned {status}: {message}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_curl_style_resolve_syntax() {
        let (host, addr) = parse_resolve("microtak-server:8443:127.0.0.1").unwrap();
        assert_eq!(host, "microtak-server");
        assert_eq!(addr, "127.0.0.1:8443".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn parses_resolve_with_ipv6_address() {
        // splitn(3, ':') on an IPv6 literal works because the address is
        // always the *last* segment and is taken verbatim, not re-split.
        let (host, addr) = parse_resolve("microtak-server:8443:::1").unwrap();
        assert_eq!(host, "microtak-server");
        assert_eq!(addr, "[::1]:8443".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn rejects_resolve_with_too_few_parts() {
        assert!(parse_resolve("microtak-server:8443").is_err());
        assert!(parse_resolve("microtak-server").is_err());
    }

    #[test]
    fn rejects_resolve_with_an_invalid_address() {
        assert!(parse_resolve("microtak-server:8443:not-an-ip").is_err());
    }

    /// URL-reserved characters (spaces, `%`, `/`) in a mission name must
    /// round-trip -- this project's own server-side test coverage
    /// (TC-MARTI-03) exists specifically because a reference
    /// implementation got this wrong, so the CLI's own encoding needs the
    /// same scrutiny, not just "looks plausible."
    #[test]
    fn urlencode_handles_reserved_characters() {
        assert_eq!(urlencode("Recon Alpha"), "Recon%20Alpha");
        assert_eq!(urlencode("50%"), "50%25");
        assert_eq!(urlencode("a/b"), "a%2Fb");
        assert_eq!(urlencode("plain-name_1.2~3"), "plain-name_1.2~3");
    }
}
