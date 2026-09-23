//! `microtak-admin-cli`: manage a MicroTAK server's device enrollment,
//! enrollment invite tokens, and mission roles from the command line --
//! wraps the same HTTP endpoints described in the standalone
//! `microtak-server` repo's `docs/ARCHITECTURE.md` ("Enrollment lockdown /
//! admin API" and "Mission roles" sections), so an operator doesn't have
//! to hand-craft `curl`/mTLS invocations for routine access management.

mod cli;
mod client;
mod enroll;
mod qr;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command, MissionCommand, TokenCommand};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Enroll(args) => enroll::run(args).await,
        Command::Token { command } => run_token_command(command).await,
        Command::Mission { command } => run_mission_command(command).await,
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
        } => {
            let client = client::build_client(&conn)?;
            let token = client::mint_token(&client, &conn, expires_in_secs, note).await?;
            println!("{token}");
            if qr {
                let url = enrollment_url
                    .as_deref()
                    .unwrap_or("<set --enrollment-url to embed it in the QR code>");
                let payload = qr::enrollment_payload(url, &token);
                qr::print_terminal_qr(&payload)?;
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
    }
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
