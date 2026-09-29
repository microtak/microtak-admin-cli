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
    /// Manage groups ("channels"): who receives what. See the README.
    Group {
        #[command(subcommand)]
        command: GroupCommand,
    },
}

/// Groups to put a device into when it enrolls -- the official TAK
/// Server's user-file lists. Each flag is repeatable.
#[derive(Args, Clone, Default)]
pub struct GroupArgs {
    /// Group to join both ways (send and receive).
    #[arg(long = "group", value_name = "GROUP")]
    pub groups: Vec<String>,
    /// Group the device may only send into (IN).
    #[arg(long = "group-in", value_name = "GROUP")]
    pub groups_in: Vec<String>,
    /// Group the device only receives from (OUT).
    #[arg(long = "group-out", value_name = "GROUP")]
    pub groups_out: Vec<String>,
}

#[derive(Subcommand)]
pub enum GroupCommand {
    /// List groups, their bitpos and members.
    List {
        #[command(flatten)]
        conn: AdminConnection,
    },
    /// Create a group.
    Create {
        #[command(flatten)]
        conn: AdminConnection,
        name: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// Delete a group and all its memberships.
    Delete {
        #[command(flatten)]
        conn: AdminConnection,
        name: String,
    },
    /// Add an identity (a device's certificate name) to a group, or change
    /// its direction.
    SetMember {
        #[command(flatten)]
        conn: AdminConnection,
        group: String,
        identity: String,
        /// in = may send into the group, out = receives from it, both.
        #[arg(long, value_enum, default_value_t = DirectionArg::Both)]
        direction: DirectionArg,
    },
    /// Remove an identity from a group.
    RemoveMember {
        #[command(flatten)]
        conn: AdminConnection,
        group: String,
        identity: String,
    },
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq, Debug)]
pub enum DirectionArg {
    In,
    Out,
    Both,
}

impl DirectionArg {
    pub fn as_api(self) -> &'static str {
        match self {
            DirectionArg::In => "IN",
            DirectionArg::Out => "OUT",
            DirectionArg::Both => "BOTH",
        }
    }
}

#[derive(Args)]
pub struct EnrollArgs {
    /// Base URL of the enrollment endpoint -- HTTPS only, e.g.
    /// https://microtak.example.com:8446
    #[arg(long, env = "MICROTAK_ADMIN_ENROLLMENT_URL")]
    pub enrollment_url: String,
    /// The server's CA certificate (PEM) to verify the enrollment endpoint
    /// against -- `ca-cert.pem` from the server's data directory. Not
    /// needed if the endpoint has a publicly-trusted certificate (e.g.
    /// Let's Encrypt), in which case the system's roots are used.
    #[arg(long, env = "MICROTAK_ADMIN_CA")]
    pub ca: Option<String>,
    /// Resolve the enrollment URL's hostname to this address instead of
    /// DNS -- curl's `--resolve` syntax (`hostname:port:address`). Handy to
    /// use a name the server certificate carries (e.g. `microtak-server`)
    /// while connecting to an IP address.
    #[arg(long, value_name = "HOST:PORT:ADDRESS")]
    pub resolve: Option<String>,
    /// Also save the CA certificate returned by the server (PEM) here --
    /// what the admin commands' `--ca` needs.
    #[arg(long)]
    pub out_ca: Option<String>,
    /// The Common Name to enroll as -- this becomes the device's identity
    /// everywhere else in MicroTAK.
    #[arg(long)]
    pub cn: String,
    /// An enrollment invite token -- required by the default
    /// `enrollment_mode = "auto"`. For the admin device, the server's
    /// one-time bootstrap token (`data_dir/bootstrap-token`).
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
        /// Bind the token to this device name: it can only enroll this
        /// identity, and it's the `username` in the QR code. Required for
        /// --qr and --pdf.
        #[arg(long)]
        cn: Option<String>,
        #[command(flatten)]
        link: LinkArgs,
        #[command(flatten)]
        groups: GroupArgs,
        /// Also render a QR code in the terminal: the standard TAK
        /// enrollment link (`tak://com.atakmap.app/enroll?...`) that
        /// ATAK/OmniTAK scan to enroll and connect on their own.
        #[arg(long)]
        qr: bool,
        /// Also write a printable enrollment handout PDF (QR code plus
        /// type-it-by-hand steps for the TAK client) to this path.
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
        /// The device name the token was minted for (`token mint --cn`).
        #[arg(long)]
        cn: String,
        #[command(flatten)]
        link: LinkArgs,
        /// A free-text note to show on the handout.
        #[arg(long)]
        note: Option<String>,
        /// Where to write the PDF.
        #[arg(long, default_value = "enrollment.pdf")]
        out: String,
    },
}

/// Where a device should enroll and connect -- what goes into the QR code
/// and the handout.
#[derive(Args, Clone)]
pub struct LinkArgs {
    /// The server's enrollment endpoint as devices reach it, e.g.
    /// https://192.168.1.10:8446, or https://tak.example.com behind a
    /// reverse proxy on 443. Required for --qr/--pdf.
    #[arg(long, env = "MICROTAK_ADMIN_ENROLLMENT_URL")]
    pub enrollment_url: Option<String>,
    /// The streaming (mTLS CoT) port devices connect to.
    #[arg(long, default_value_t = crate::qr::DEFAULT_STREAMING_PORT)]
    pub streaming_port: u16,
    /// The Marti API port devices connect to.
    #[arg(long, default_value_t = crate::qr::DEFAULT_API_PORT)]
    pub api_port: u16,
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
        #[command(flatten)]
        groups: GroupArgs,
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
    /// Read only (official MISSION_READONLY_SUBSCRIBER).
    ReadonlySubscriber,
}

impl std::fmt::Display for RoleArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RoleArg::Owner => "owner",
            RoleArg::Subscriber => "subscriber",
            RoleArg::ReadonlySubscriber => "readonly_subscriber",
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
