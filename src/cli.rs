//! Command-line argument definitions.

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "microtak-admin-cli",
    version,
    about = "Manage a MicroTAK server: device enrollment, enrollment invite tokens, and mission roles."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Shared connection settings for every subcommand that talks to the Marti
/// API over mTLS (everything except `enroll`, which talks to the plain
/// unauthenticated enrollment endpoint instead -- it has no cert yet).
#[derive(Args, Clone)]
pub struct AdminConnection {
    /// Base URL of the Marti API, e.g. https://microtak.example.com:8443
    #[arg(long, env = "MICROTAK_ADMIN_SERVER")]
    pub server: String,
    /// Path to the admin device's client certificate (PEM).
    #[arg(long, env = "MICROTAK_ADMIN_CERT")]
    pub cert: String,
    /// Path to the admin device's private key (PEM).
    #[arg(long, env = "MICROTAK_ADMIN_KEY")]
    pub key: String,
    /// Path to the server's CA certificate (PEM) -- from
    /// `GET /Marti/api/tls/ca.pem` on first setup.
    #[arg(long, env = "MICROTAK_ADMIN_CA")]
    pub ca: String,
    /// Resolve the server URL's hostname to this address instead of using
    /// normal DNS -- curl's own `--resolve` syntax
    /// (`hostname:port:address`), useful when connecting directly to an IP
    /// before DNS is set up (a real, not just a testing, scenario for a
    /// grid-down / local-network deployment).
    #[arg(long, value_name = "HOST:PORT:ADDRESS")]
    pub resolve: Option<String>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Enroll a new device: generate a keypair and CSR, submit it to the
    /// server's (unauthenticated) enrollment endpoint, and save the signed
    /// certificate and key to disk.
    Enroll(EnrollArgs),
    /// Manage enrollment invite tokens.
    Token {
        #[command(subcommand)]
        command: TokenCommand,
    },
    /// Manage password accounts (for clients that expect real
    /// username/password login, e.g. CloudTAK, rather than a bare
    /// enrollment token).
    User {
        #[command(subcommand)]
        command: UserCommand,
    },
    /// Manage mission roles.
    Mission {
        #[command(subcommand)]
        command: MissionCommand,
    },
}

#[derive(Args)]
pub struct EnrollArgs {
    /// Base URL of the plain (unauthenticated) enrollment endpoint, e.g.
    /// http://microtak.example.com:8446
    #[arg(long, env = "MICROTAK_ADMIN_ENROLLMENT_URL")]
    pub enrollment_url: String,
    /// The Common Name to enroll as -- this becomes the device's identity
    /// everywhere else in MicroTAK.
    #[arg(long)]
    pub cn: String,
    /// An enrollment invite token, required once the server has locked
    /// enrollment down (the default `enrollment_mode = "auto"` does this
    /// automatically once its configured admin device has enrolled).
    #[arg(long)]
    pub token: Option<String>,
    /// Where to write the signed certificate (PEM).
    #[arg(long, default_value = "device.pem")]
    pub out_cert: String,
    /// Where to write the private key (PEM).
    #[arg(long, default_value = "device.key")]
    pub out_key: String,
}

#[derive(Subcommand)]
pub enum TokenCommand {
    /// Mint a new enrollment invite token.
    Mint {
        #[command(flatten)]
        conn: AdminConnection,
        /// Seconds until the token expires. Omit for a token that never
        /// expires.
        #[arg(long)]
        expires_in_secs: Option<i64>,
        /// A free-text note to help you remember what this token is for.
        #[arg(long)]
        note: Option<String>,
        /// Also render the token as a QR code in the terminal, encoding
        /// the enrollment URL and token together so a device operator can
        /// scan it instead of copy-pasting -- see the README for the exact
        /// payload format.
        #[arg(long)]
        qr: bool,
        /// The enrollment endpoint URL to embed in the QR code payload.
        /// Only used with --qr and/or --pdf.
        #[arg(long, env = "MICROTAK_ADMIN_ENROLLMENT_URL")]
        enrollment_url: Option<String>,
        /// Also write a printable enrollment handout PDF (QR code plus
        /// manual, type-it-by-hand enrollment steps) to this path.
        /// Requires --enrollment-url, since the handout needs a real URL
        /// to put in both the QR code and the manual instructions.
        #[arg(long, value_name = "PATH")]
        pdf: Option<String>,
    },
    /// List all enrollment tokens and their status.
    List {
        #[command(flatten)]
        conn: AdminConnection,
    },
    /// Revoke a token so it can never be used, even if never consumed.
    Revoke {
        #[command(flatten)]
        conn: AdminConnection,
        token: String,
    },
    /// Generate a printable enrollment handout PDF for a token you already
    /// have (e.g. from `token list`), without minting a new one or
    /// contacting the server at all.
    Pdf {
        token: String,
        /// The enrollment endpoint URL to embed in the QR code and the
        /// manual enrollment steps.
        #[arg(long, env = "MICROTAK_ADMIN_ENROLLMENT_URL")]
        enrollment_url: String,
        /// A free-text note to show on the handout.
        #[arg(long)]
        note: Option<String>,
        /// Where to write the PDF.
        #[arg(long, default_value = "enrollment.pdf")]
        out: String,
    },
}

#[derive(Subcommand)]
pub enum UserCommand {
    /// Mint a new password account. Omit --password to have the server
    /// generate a random one, printed once.
    Mint {
        #[command(flatten)]
        conn: AdminConnection,
        username: String,
        #[arg(long)]
        password: Option<String>,
    },
    /// List all password accounts and their status.
    List {
        #[command(flatten)]
        conn: AdminConnection,
    },
    /// Revoke a password account -- it can no longer log in or self-enroll
    /// a device cert, even with the correct password.
    Revoke {
        #[command(flatten)]
        conn: AdminConnection,
        username: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum RoleArg {
    Owner,
    Subscriber,
}

impl std::fmt::Display for RoleArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RoleArg::Owner => "owner",
            RoleArg::Subscriber => "subscriber",
        })
    }
}

#[derive(Subcommand)]
pub enum MissionCommand {
    /// List all missions.
    List {
        #[command(flatten)]
        conn: AdminConnection,
    },
    /// Show one mission, including its current role assignments.
    Get {
        #[command(flatten)]
        conn: AdminConnection,
        name: String,
    },
    /// Assign (or change) an identity's role on a mission. You must
    /// already hold the Owner role on it yourself.
    SetRole {
        #[command(flatten)]
        conn: AdminConnection,
        mission: String,
        uid: String,
        #[arg(value_enum)]
        role: RoleArg,
    },
    /// Revoke an identity's role on a mission entirely. You must already
    /// hold the Owner role on it yourself; rejected if it would leave the
    /// mission with zero owners.
    RevokeRole {
        #[command(flatten)]
        conn: AdminConnection,
        mission: String,
        uid: String,
    },
}
