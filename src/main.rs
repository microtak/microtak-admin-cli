//! `microtak-admin-cli`: manage a MicroTAK server's device enrollment,
//! enrollment invite tokens, and mission roles from the command line --
//! wraps the same HTTP endpoints described in the standalone
//! `microtak-server` repo's `docs/ARCHITECTURE.md` ("Enrollment lockdown /
//! admin API" and "Mission roles" sections), so an operator doesn't have
//! to hand-craft `curl`/mTLS invocations for routine access management.

mod cli;
mod client;
mod enroll;
mod pdf;
mod qr;

use anyhow::{Context, Result};
use clap::Parser;

use cli::{Cli, Command, MissionCommand, TokenCommand, UserCommand};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Enroll(args) => enroll::run(args).await,
        Command::Token { command } => run_token_command(command).await,
        Command::User { command } => run_user_command(command).await,
        Command::Mission { command } => run_mission_command(command).await,
    }
}

async fn run_user_command(command: UserCommand) -> Result<()> {
    match command {
        UserCommand::Mint { conn, username, password } => {
            let client = client::build_client(&conn)?;
            let password = client::mint_user(&client, &conn, &username, password).await?;
            println!("Minted user '{username}' with password: {password}");
            Ok(())
        }
        UserCommand::List { conn } => {
            let client = client::build_client(&conn)?;
            let users = client::list_users(&client, &conn).await?;
            print_user_table(&users);
            Ok(())
        }
        UserCommand::Revoke { conn, username } => {
            let client = client::build_client(&conn)?;
            client::revoke_user(&client, &conn, &username).await?;
            println!("Revoked.");
            Ok(())
        }
    }
}

fn print_user_table(users: &[client::UserInfo]) {
    if users.is_empty() {
        println!("No user accounts.");
        return;
    }
    println!("{:<30} {:<10} CREATED", "USERNAME", "STATUS");
    for user in users {
        let status = if user.revoked { "revoked" } else { "active" };
        println!("{:<30} {:<10} {}", user.username, status, format_unix(user.created_at_unix));
    }
}

async fn run_token_command(command: TokenCommand) -> Result<()> {
    match command {
        TokenCommand::Mint {
            conn,
            expires_in_secs,
            note,
            qr,
            enrollment_url,
            pdf,
        } => {
            let client = client::build_client(&conn)?;
            let note_for_pdf = note.clone();
            let token = client::mint_token(&client, &conn, expires_in_secs, note).await?;
            println!("{token}");
            if qr {
                let url = enrollment_url
                    .as_deref()
                    .unwrap_or("<set --enrollment-url to embed it in the QR code>");
                let payload = qr::enrollment_payload(url, &token);
                qr::print_terminal_qr(&payload)?;
            }
            if let Some(out_path) = pdf {
                let url = enrollment_url
                    .as_deref()
                    .context("--pdf requires --enrollment-url so the handout has a real URL to show")?;
                let expires_at = match expires_in_secs {
                    Some(secs) => pdf::Expiry::At(format_unix(now_unix() + secs)),
                    None => pdf::Expiry::Never,
                };
                let info = pdf::HandoutInfo {
                    token: &token,
                    enrollment_url: url,
                    note: note_for_pdf.as_deref(),
                    expires_at,
                };
                pdf::write_enrollment_pdf(&info, &out_path)?;
                println!("Wrote enrollment handout to {out_path}");
            }
            Ok(())
        }
        TokenCommand::List { conn } => {
            let client = client::build_client(&conn)?;
            let tokens = client::list_tokens(&client, &conn).await?;
            print_token_table(&tokens);
            Ok(())
        }
        TokenCommand::Revoke { conn, token } => {
            let client = client::build_client(&conn)?;
            client::revoke_token(&client, &conn, &token).await?;
            println!("Revoked.");
            Ok(())
        }
        TokenCommand::Pdf { token, enrollment_url, note, out } => {
            let info = pdf::HandoutInfo {
                token: &token,
                enrollment_url: &enrollment_url,
                note: note.as_deref(),
                // No server round trip here (that's the point of this
                // subcommand -- see its help text), so real status/expiry
                // isn't known; shown plainly rather than guessed at.
                expires_at: pdf::Expiry::Unknown,
            };
            pdf::write_enrollment_pdf(&info, &out)?;
            println!("Wrote enrollment handout to {out}");
            Ok(())
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn run_mission_command(command: MissionCommand) -> Result<()> {
    match command {
        MissionCommand::List { conn } => {
            let client = client::build_client(&conn)?;
            let missions = client::list_missions(&client, &conn).await?;
            for mission in missions {
                println!("{}  (creator: {})", mission.name, mission.creator_uid);
            }
            Ok(())
        }
        MissionCommand::Get { conn, name } => {
            let client = client::build_client(&conn)?;
            let mission = client::get_mission(&client, &conn, &name).await?;
            print_mission(&mission);
            Ok(())
        }
        MissionCommand::SetRole {
            conn,
            mission,
            uid,
            role,
        } => {
            let client = client::build_client(&conn)?;
            client::set_role(&client, &conn, &mission, &uid, role).await?;
            println!("Assigned {role} to '{uid}' on '{mission}'.");
            Ok(())
        }
        MissionCommand::RevokeRole { conn, mission, uid } => {
            let client = client::build_client(&conn)?;
            client::revoke_role(&client, &conn, &mission, &uid).await?;
            println!("Revoked the role '{uid}' held on '{mission}'.");
            Ok(())
        }
    }
}

fn print_token_table(tokens: &[client::EnrollmentToken]) {
    if tokens.is_empty() {
        println!("No enrollment tokens.");
        return;
    }
    println!(
        "{:<66} {:<10} {:<20} {:<20} {:<20} NOTE",
        "TOKEN", "STATUS", "USED BY", "CREATED", "EXPIRES"
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    for token in tokens {
        let status = if token.revoked {
            "revoked"
        } else if token.used {
            "used"
        } else if token.expires_at_unix.is_some_and(|expires| now >= expires) {
            "expired"
        } else {
            "active"
        };
        let expires = match token.expires_at_unix {
            Some(expires) => format_unix(expires),
            None => "never".to_string(),
        };
        println!(
            "{:<66} {:<10} {:<20} {:<20} {:<20} {}",
            token.token,
            status,
            token.used_by_common_name.as_deref().unwrap_or("-"),
            format_unix(token.created_at_unix),
            expires,
            token.note.as_deref().unwrap_or("-"),
        );
    }
}

fn format_unix(unix_seconds: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(unix_seconds)
        .ok()
        .and_then(|dt| {
            dt.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| unix_seconds.to_string())
}

fn print_mission(mission: &client::Mission) {
    println!("Name:        {}", mission.name);
    println!("Description: {}", mission.description.as_deref().unwrap_or("-"));
    println!("Creator:     {}", mission.creator_uid);
    println!("Subscribers: {}", mission.subscribers.join(", "));
    println!("Roles:");
    for (uid, role) in &mission.roles {
        println!("  {uid:<30} {role}");
    }
}
