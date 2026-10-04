//! SPEC-002 AC-010 *(Windows)* — After a successful enrollment, the
//! persisted `cert.pem`, `key.dat`, and `identity.json` have filesystem
//! ACLs that exclude all principals other than the owner and SYSTEM, each
//! with full control (NFR-003). `#[cfg(windows)]`.
//!
//! The DACL is read with PowerShell's `Get-Acl`, asking for every
//! identity as a full SID (`SecurityIdentifier` never abbreviates; SDDL
//! writes the RID-500 account as `LA`, and principal names are
//! localized). It must be protected (no inherited entries) and hold
//! exactly two rules, the current user's SID and SYSTEM (S-1-5-18), each
//! Allow, FullControl and not inherited. The same code runs locally and
//! on the CI runner.
//!
//! A second test gives the artifacts an explicit entry for another
//! principal (BUILTIN\Users, S-1-5-32-545) before they are persisted: the
//! hardening must remove it, not only the inherited entries.

mod common;

/// The current user's SID, from `whoami /user /fo csv /nh`
/// (`"domain\user","S-1-5-21-..."`, always the full form).
#[cfg(windows)]
fn current_user_sid() -> String {
    let output = std::process::Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .expect("failed to run whoami");
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .rsplit(',')
        .next()
        .map(|field| field.trim_matches('"').to_string())
        .filter(|sid| sid.starts_with("S-1-"))
        .unwrap_or_else(|| panic!("no SID in whoami output: {text}"))
}

/// One access rule as `Get-Acl` reports it, with the identity as a SID.
#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    sid: String,
    kind: String,
    rights: String,
    inherited: String,
}

/// The DACL of `path`: whether it is protected, and its access rules.
#[cfg(windows)]
fn read_dacl(path: &std::path::Path) -> (bool, Vec<Rule>) {
    const SCRIPT: &str = "$a = Get-Acl -LiteralPath $env:CG_ACL_PATH; \
        'PROTECTED=' + $a.AreAccessRulesProtected; \
        $a.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier]) | \
        ForEach-Object { $_.IdentityReference.Value + '|' + $_.AccessControlType + '|' + \
        $_.FileSystemRights + '|' + $_.IsInherited }";
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("CG_ACL_PATH", path)
        .output()
        .expect("failed to run powershell");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "Get-Acl failed on {}: {text}{}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut protected = None;
    let mut rules = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(value) = line.strip_prefix("PROTECTED=") {
            protected = Some(value == "True");
            continue;
        }
        let fields: Vec<&str> = line.split('|').collect();
        assert_eq!(fields.len(), 4, "unexpected Get-Acl line: {line}");
        rules.push(Rule {
            sid: fields[0].to_string(),
            kind: fields[1].to_string(),
            rights: fields[2].to_string(),
            inherited: fields[3].to_string(),
        });
    }
    let protected = protected.unwrap_or_else(|| panic!("no PROTECTED line: {text}"));
    (protected, rules)
}

/// Assert the DACL is protected and holds exactly two rules, the current
/// user's SID and SYSTEM, each Allow, FullControl and not inherited.
#[cfg(windows)]
fn assert_owner_only_dacl(path: &std::path::Path, user_sid: &str) {
    let (protected, rules) = read_dacl(path);
    assert!(
        protected,
        "DACL on {} is not protected (inherited entries possible): {rules:?}",
        path.display()
    );
    let mut sids: Vec<&str> = rules.iter().map(|r| r.sid.as_str()).collect();
    sids.sort_unstable();
    let mut expected = vec!["S-1-5-18", user_sid];
    expected.sort_unstable();
    assert_eq!(
        sids,
        expected,
        "DACL on {} must hold exactly the current user and SYSTEM: {rules:?}",
        path.display()
    );
    for rule in &rules {
        assert_eq!(
            (
                rule.kind.as_str(),
                rule.rights.as_str(),
                rule.inherited.as_str()
            ),
            ("Allow", "FullControl", "False"),
            "every rule on {} must be Allow, FullControl, not inherited: {rules:?}",
            path.display()
        );
    }
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_010_owner_only_acls() {
    let mock = common::MockServer::start().await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-ac-010");

    cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("enrollment should succeed");

    let user_sid = current_user_sid();
    assert_owner_only_dacl(&fixture.cert_path, &user_sid);
    assert_owner_only_dacl(&fixture.key_path, &user_sid);
    assert_owner_only_dacl(&fixture.identity_path, &user_sid);
}

/// An explicit entry for another principal on the artifacts before they
/// are persisted (BUILTIN\Users, read) is gone afterwards: the hardening
/// replaces the whole DACL, not only the inherited entries.
#[cfg(windows)]
#[test]
fn enroll_ac_010_hardening_removes_other_explicit_entries() {
    const USERS_SID: &str = "S-1-5-32-545";
    let fixture = common::enrollment_fixture("http://127.0.0.1:9", "tok-ac-010-explicit");
    let paths = [
        &fixture.cert_path,
        &fixture.key_path,
        &fixture.identity_path,
    ];
    for path in paths {
        std::fs::write(path, b"placeholder").expect("pre-create artifact");
        let status = std::process::Command::new("icacls")
            .arg(path)
            .arg("/grant")
            .arg(format!("*{USERS_SID}:(R)"))
            .stdout(std::process::Stdio::null())
            .status()
            .expect("failed to run icacls");
        assert!(
            status.success(),
            "icacls /grant failed on {}",
            path.display()
        );
        let (_, before) = read_dacl(path);
        assert!(
            before
                .iter()
                .any(|r| r.sid == USERS_SID && r.inherited == "False"),
            "setup: an explicit Users entry on {}: {before:?}",
            path.display()
        );
    }

    let keypair = cg_agent::crypto::AgentKeypair::from_secret_bytes(&[7u8; 32]);
    let enrolled = cg_agent::enrollment::EnrolledIdentity {
        agent_id: common::TEST_AGENT_ID.to_string(),
        client_certificate_pem: common::TEST_CLIENT_CERT_PEM.to_string(),
        issued_at: "2026-10-04T00:00:00.000Z".to_string(),
        expires_at: "2027-01-02T00:00:00.000Z".to_string(),
        secret_seed: keypair.secret_seed(),
        public_key: keypair.public_key_bytes(),
    };
    cg_agent::identity::persist_identity(&fixture.config, &enrolled)
        .expect("persist and harden the artifacts");

    let user_sid = current_user_sid();
    for path in paths {
        assert_owner_only_dacl(path, &user_sid);
    }
}
