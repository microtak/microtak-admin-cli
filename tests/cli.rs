//! End-to-end: the real `microtak-admin-cli` binary against a real,
//! running `microtak_server::app::App` (a dev-dependency on the sibling
//! `../microtak-server` checkout, as microtak-admin-web's e2e test does).
//! Every command group is run the way an operator runs it -- enroll with
//! the bootstrap token, then tokens, accounts, groups, missions and
//! devices -- and checked against what the server now holds.

use std::path::{Path, PathBuf};
use std::process::Command;

use microtak_server::app::{App, AppConfig};

const NAME: &str = "microtak-server";

struct Server {
    dir: PathBuf,
    ca: PathBuf,
    enroll_port: u16,
    api_port: u16,
    missions: std::sync::Arc<microtak_server::missions::MissionStore>,
    bootstrap: String,
}

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "microtak-admin-cli-e2e-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn start(label: &str) -> Server {
    let dir = temp_dir(label);
    let config = AppConfig {
        enrollment_addr: "127.0.0.1:0".parse().unwrap(),
        marti_api_addr: "127.0.0.1:0".parse().unwrap(),
        plain_tcp_addr: "127.0.0.1:0".parse().unwrap(),
        mtls_addr: "127.0.0.1:0".parse().unwrap(),
        data_dir: dir.join("data"),
        admin_common_name: Some("cli-admin".to_string()),
        server_names_from_interfaces: false,
        ..AppConfig::default()
    };
    let app = App::bind(config).await.unwrap();
    let ca = dir.join("ca.pem");
    std::fs::write(&ca, &app.ca_cert_pem).unwrap();
    let server = Server {
        enroll_port: app.enrollment_addr().unwrap().port(),
        api_port: app.marti_api_addr().unwrap().port(),
        missions: app.missions.clone(),
        bootstrap: std::fs::read_to_string(app.bootstrap_token_path().unwrap())
            .unwrap()
            .trim()
            .to_string(),
        dir,
        ca,
    };
    tokio::spawn(app.run());
    server
}

fn as_strs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// Runs the CLI; returns (success, stdout, stderr).
fn cli(args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_microtak-admin-cli"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn ok(args: &[&str]) -> String {
    let (success, stdout, stderr) = cli(args);
    assert!(success, "{args:?} failed:\n{stdout}\n{stderr}");
    stdout
}

impl Server {
    fn enroll_args(&self, cn: &str, token: &str, out: &Path) -> Vec<String> {
        vec![
            "enroll".into(),
            "--enrollment-url".into(),
            format!("https://{NAME}:{}", self.enroll_port),
            "--resolve".into(),
            format!("{NAME}:{}:127.0.0.1", self.enroll_port),
            "--ca".into(),
            self.ca.display().to_string(),
            "--cn".into(),
            cn.into(),
            "--token".into(),
            token.into(),
            "--out-cert".into(),
            out.join(format!("{cn}.pem")).display().to_string(),
            "--out-key".into(),
            out.join(format!("{cn}.key")).display().to_string(),
        ]
    }

    /// The admin connection flags for `cn`'s credentials.
    fn conn(&self, cn: &str) -> Vec<String> {
        vec![
            "--server".into(),
            format!("https://{NAME}:{}", self.api_port),
            "--resolve".into(),
            format!("{NAME}:{}:127.0.0.1", self.api_port),
            "--cert".into(),
            self.dir.join(format!("{cn}.pem")).display().to_string(),
            "--key".into(),
            self.dir.join(format!("{cn}.key")).display().to_string(),
            "--ca".into(),
            self.ca.display().to_string(),
        ]
    }

    fn admin(&self, command: &[&str]) -> (bool, String, String) {
        let conn = self.conn("cli-admin");
        let args: Vec<&str> = command
            .iter()
            .copied()
            .chain(conn.iter().map(String::as_str))
            .collect();
        cli(&args)
    }

    fn admin_ok(&self, command: &[&str]) -> String {
        let (success, stdout, stderr) = self.admin(command);
        assert!(success, "{command:?} failed:\n{stdout}\n{stderr}");
        stdout
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_command_group_against_a_real_server() {
    let s = start("all").await;

    // Enroll the admin with the bootstrap token; the key is owner-only.
    let enroll = s.enroll_args("cli-admin", &s.bootstrap, &s.dir);
    let out = ok(&as_strs(&enroll));
    assert!(out.contains("Enrolled 'cli-admin'"), "{out}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(s.dir.join("cli-admin.key"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    // The bootstrap token works once.
    let (again, _, _) = cli(&as_strs(&s.enroll_args(
        "cli-admin",
        &s.bootstrap,
        &temp_dir("again"),
    )));
    assert!(!again, "a used bootstrap token is refused");

    // Tokens: mint (bound to a device name), list, enroll with it, revoke.
    let token = s
        .admin_ok(&["token", "mint", "--cn", "dev-1", "--note", "field kit"])
        .trim()
        .to_string();
    assert!(!token.is_empty());
    let listed = s.admin_ok(&["token", "list"]);
    assert!(
        listed.contains("dev-1") && listed.contains("field kit"),
        "{listed}"
    );
    ok(&as_strs(&s.enroll_args("dev-1", &token, &s.dir)));
    let other = s.admin_ok(&["token", "mint"]).trim().to_string();
    s.admin_ok(&["token", "revoke", &other]);
    let listed = s.admin_ok(&["token", "list"]);
    assert!(
        listed.contains("used") && listed.contains("revoked"),
        "{listed}"
    );
    let (revoked_works, _, _) = cli(&as_strs(&s.enroll_args("dev-2", &other, &temp_dir("dev2"))));
    assert!(!revoked_works, "a revoked token can't enroll");

    // Password accounts.
    s.admin_ok(&[
        "user",
        "mint",
        "alice",
        "--password",
        "correct horse battery staple",
    ]);
    assert!(s.admin_ok(&["user", "list"]).contains("alice"));
    s.admin_ok(&["user", "revoke", "alice"]);
    assert!(s.admin_ok(&["user", "list"]).contains("revoked"));

    // Groups.
    s.admin_ok(&["group", "create", "Red"]);
    s.admin_ok(&["group", "set-member", "Red", "dev-1", "--direction", "in"]);
    let groups = s.admin_ok(&["group", "list"]);
    assert!(
        groups.contains("Red") && groups.contains("dev-1"),
        "{groups}"
    );
    s.admin_ok(&["group", "remove-member", "Red", "dev-1"]);
    s.admin_ok(&["group", "delete", "Red"]);
    assert!(!s.admin_ok(&["group", "list"]).contains("Red"));

    // Missions: one with a default role, a subscribed device.
    let mission = s
        .missions
        .create(
            microtak_server::missions::NewMission {
                guid: "6f9619ff-8b86-d011-b42d-00cf4fc964ff".into(),
                name: "Recon".into(),
                fields: Default::default(),
                tool: "public".into(),
                creator_uid: "ANDROID-admin".into(),
                groups: ["__ANON__".to_string()].into_iter().collect(),
                password_hash: None,
                default_role: Some(microtak_server::missions::Role::Subscriber),
                invite_only: false,
            },
            1_000,
        )
        .unwrap();
    s.missions
        .subscribe(
            &mission.guid,
            "ANDROID-dev",
            "dev-1",
            microtak_server::missions::Role::Subscriber,
            1_000,
            |_| "t".into(),
        )
        .unwrap();
    assert!(s.admin_ok(&["mission", "list"]).contains("Recon"));
    let detail = s.admin_ok(&["mission", "get", "Recon"]);
    assert!(
        detail.contains("ANDROID-dev") && detail.contains("MISSION_SUBSCRIBER"),
        "{detail}"
    );
    s.admin_ok(&["mission", "set-role", "Recon", "ANDROID-dev", "owner"]);
    assert!(
        s.admin_ok(&["mission", "get", "Recon"])
            .contains("MISSION_OWNER")
    );
    s.admin_ok(&["mission", "revoke-role", "Recon", "ANDROID-dev"]);
    assert!(
        !s.admin_ok(&["mission", "get", "Recon"])
            .contains("ANDROID-dev")
    );
    let (unknown, _, err) = s.admin(&["mission", "get", "Nope"]);
    assert!(!unknown, "{err}");

    // Devices: list, revoke the device's certificate, block and unblock.
    assert!(s.admin_ok(&["device", "list"]).contains("dev-1"));
    s.admin_ok(&["device", "revoke-cert", "dev-1"]);
    let (twice, _, err) = s.admin(&["device", "revoke-cert", "dev-1"]);
    assert!(!twice && err.contains("no unrevoked certificate"), "{err}");
    s.admin_ok(&["device", "block", "dev-1"]);
    s.admin_ok(&["device", "unblock", "dev-1"]);

    // The admin connection is verified: a wrong CA is refused.
    let wrong_ca = s.dir.join("wrong-ca.pem");
    std::fs::write(
        &wrong_ca,
        rcgen::generate_simple_self_signed(vec!["x".into()])
            .unwrap()
            .cert
            .pem(),
    )
    .unwrap();
    let mut conn = s.conn("cli-admin");
    let ca_at = conn.iter().position(|a| a == "--ca").unwrap() + 1;
    conn[ca_at] = wrong_ca.display().to_string();
    let args: Vec<&str> = ["token", "list"]
        .into_iter()
        .chain(conn.iter().map(String::as_str))
        .collect();
    let (trusted, _, _) = cli(&args);
    assert!(!trusted, "a server outside --ca is refused");
}

#[test]
fn plain_http_enrollment_is_refused() {
    let (success, _, stderr) = cli(&[
        "enroll",
        "--enrollment-url",
        "http://localhost:8446",
        "--cn",
        "x",
        "--token",
        "t",
    ]);
    assert!(!success && stderr.contains("HTTPS only"), "{stderr}");
}
