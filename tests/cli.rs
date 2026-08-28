//! End-to-end tests for the `dedalo` binary.
//!
//! These run the real executable against real repositories. They exist to pin
//! down two contracts that are easy to break by accident: the exit codes a
//! pipeline branches on, and the shape of `--json`, which `action.yml` parses
//! to produce its outputs.

use assert_cmd::Command;
use dedalo::testing::TempRepo;
use predicates::str::contains;
use serde_json::Value;
use std::path::Path;

/// A repository with `dedalo.toml` and two linked contributors.
fn project() -> TempRepo {
    let repo = TempRepo::new("cli");
    repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 30);
    repo.merge_feature("feature-b", ("Bea", "bea@example.com"), 10);

    dedalo(repo.path())
        .args([
            "init",
            "--name",
            "demo",
            "--open-collective",
            "demo-collective",
        ])
        .assert()
        .success();

    // The template ships zero addresses on purpose; tests need real-looking ones.
    let config_path = repo.path().join("dedalo.toml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    let config = config
        .replacen(
            "source = \"11111111111111111111111111111111\"",
            "source = \"So11111111111111111111111111111111111111112\"",
            1,
        )
        .replacen(
            "treasury = \"11111111111111111111111111111111\"",
            "treasury = \"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA\"",
            1,
        )
        .replacen(
            "open_collective = \"11111111111111111111111111111111\"",
            "open_collective = \"ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL\"",
            1,
        );
    std::fs::write(&config_path, config).unwrap();

    for (handle, wallet, email) in [
        (
            "ada",
            "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
            "ada@example.com",
        ),
        (
            "bea",
            "MerkS3LaQBSvM5JZsvBaLZBBSMvMB5aTuLRHrvKAyDo",
            "bea@example.com",
        ),
    ] {
        dedalo(repo.path())
            .args(["identity", "link", handle, wallet, "--email", email])
            .assert()
            .success();
    }
    repo
}

fn dedalo(repo: &Path) -> Command {
    let mut cmd = Command::cargo_bin("dedalo").expect("the binary must be built");
    cmd.arg("-C").arg(repo);
    // Keep output deterministic regardless of the developer's terminal.
    cmd.env("NO_COLOR", "1");
    cmd
}

fn json_of(repo: &Path, args: &[&str]) -> Value {
    let output = dedalo(repo).arg("--json").args(args).assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "`dedalo --json {}` did not emit JSON: {e}\n{stdout}",
            args.join(" ")
        )
    })
}

#[test]
fn init_creates_a_usable_config_and_refuses_to_clobber_it() {
    let repo = TempRepo::new("init");

    dedalo(repo.path()).arg("init").assert().success();
    assert!(repo.path().join("dedalo.toml").is_file());

    dedalo(repo.path())
        .arg("init")
        .assert()
        .failure()
        .stderr(contains("already exists"));

    dedalo(repo.path())
        .args(["init", "--force"])
        .assert()
        .success();
}

#[test]
fn commands_outside_a_project_point_at_init() {
    let repo = TempRepo::new("no-config");
    dedalo(repo.path())
        .arg("status")
        .assert()
        .failure()
        .stderr(contains("dedalo init"));
}

#[test]
fn scan_reports_the_pending_merges() {
    let repo = project();
    let merges = json_of(repo.path(), &["scan"]);
    let merges = merges.as_array().expect("scan --json must be an array");
    assert_eq!(merges.len(), 2);
    assert_eq!(merges[0]["sha"].as_str().unwrap().len(), 40);
    assert_eq!(merges[0]["commits"].as_array().unwrap().len(), 1);
}

#[test]
fn contributors_are_scored_by_the_size_of_their_work() {
    let repo = project();
    let attribution = json_of(repo.path(), &["contributors"]);
    let contributions = attribution["contributions"].as_array().unwrap();
    assert_eq!(contributions.len(), 2);
    // Ada wrote three times the lines Bea did, so she leads.
    assert_eq!(contributions[0]["author"]["email"], "ada@example.com");
    let ada = contributions[0]["score"].as_u64().unwrap();
    let bea = contributions[1]["score"].as_u64().unwrap();
    assert!(ada > bea, "ada scored {ada}, bea scored {bea}");
}

/// `action.yml` reads `"id"` and `"gross"` out of this JSON with grep. If the
/// field names or nesting change, the Action silently produces empty outputs,
/// so this test guards the contract between the two.
#[test]
fn plan_json_carries_the_fields_the_action_reads() {
    let repo = project();
    let plan = json_of(repo.path(), &["plan", "--amount", "1000"]);

    let id = plan["id"].as_str().expect("plan must expose a string id");
    assert!(id.starts_with("ded1"), "unexpected plan id format: {id}");
    assert_eq!(plan["split"]["gross"], "1000000000");

    let raw = serde_json::to_string_pretty(&plan).unwrap();
    let grepped_id = raw
        .lines()
        .find(|l| l.contains("\"id\""))
        .and_then(|l| l.split('"').nth(3))
        .unwrap();
    assert_eq!(grepped_id, id, "the Action's grep must find the same id");
}

#[test]
fn a_plan_pays_out_exactly_what_it_was_given() {
    let repo = project();
    let plan = json_of(repo.path(), &["plan", "--amount", "1000"]);

    let gross: u128 = plan["split"]["gross"].as_str().unwrap().parse().unwrap();
    let paid: u128 = plan["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["amount"].as_str().unwrap().parse::<u128>().unwrap())
        .sum();
    assert_eq!(paid, gross, "a round must not create or lose base units");

    // 2.5% of the round reaches the network's Open Collective.
    let protocol = plan["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "protocol")
        .expect("every plan funds the protocol");
    assert_eq!(protocol["amount"], "25000000");
}

#[test]
fn the_same_history_plans_to_the_same_id() {
    let repo = project();
    let first = json_of(repo.path(), &["plan", "--amount", "1000"]);
    let second = json_of(repo.path(), &["plan", "--amount", "1000"]);
    assert_eq!(first["id"], second["id"]);

    let bigger = json_of(repo.path(), &["plan", "--amount", "2000"]);
    assert_ne!(first["id"], bigger["id"]);
}

#[test]
fn a_malformed_amount_is_rejected_before_anything_happens() {
    let repo = project();
    for bad in ["-5", "abc", "1.0000001", "1e6"] {
        dedalo(repo.path())
            .args(["plan", "--amount", bad])
            .assert()
            .failure();
    }
    // Zero is a valid decimal but not a valid round.
    dedalo(repo.path())
        .args(["plan", "--amount", "0"])
        .assert()
        .failure()
        .stderr(contains("greater than zero"));
}

#[test]
fn settle_simulates_by_default_and_leaves_the_cursor_alone() {
    let repo = project();

    dedalo(repo.path())
        .args(["settle", "--amount", "1000"])
        .assert()
        .success()
        .stdout(contains("simulated"));

    let status = json_of(repo.path(), &["status"]);
    assert!(status["state"]["last_settled_commit"].is_null());
    assert_eq!(status["state"]["lifetime_paid"], "0");
    // The simulation is still recorded: a dry run is history too.
    let ledger = json_of(repo.path(), &["ledger"]);
    assert!(!ledger.as_array().unwrap().is_empty());
}

#[test]
fn unlinked_contributors_are_reported_not_hidden() {
    let repo = TempRepo::new("unlinked");
    repo.merge_feature_with_trailer(
        "feature-a",
        ("Ada", "ada@example.com"),
        20,
        Some("Co-authored-by: Cy <cy@example.com>"),
    );
    dedalo(repo.path()).arg("init").assert().success();

    let missing = json_of(repo.path(), &["identity", "missing"]);
    let emails: Vec<&str> = missing
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["author"]["email"].as_str().unwrap())
        .collect();
    assert!(
        emails.contains(&"cy@example.com"),
        "co-author must be listed: {emails:?}"
    );
    assert!(emails.contains(&"ada@example.com"));
}

/// `dedalo.toml` is hand-edited and full of explanatory comments. An automated
/// `identity link` must not silently strip them.
#[test]
fn linking_an_identity_preserves_the_config_comments() {
    let repo = project();
    let config = std::fs::read_to_string(repo.path().join("dedalo.toml")).unwrap();
    assert!(
        config.contains("# Merges into this branch are what earn a payout."),
        "toml_edit must preserve comments"
    );
    assert!(config.contains("[[identities]]"));
    // Stored checksummed, whatever case it was typed in.
    assert!(
        config.contains("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU"),
        "the linked address should be stored in its EIP-55 form:\n{config}"
    );
}

#[test]
fn identity_link_is_idempotent_and_remove_undoes_it() {
    let repo = project();

    dedalo(repo.path())
        .args([
            "identity",
            "link",
            "ada",
            "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
            "--email",
            "ada@example.com",
        ])
        .assert()
        .success();

    let identities = json_of(repo.path(), &["identity", "list"]);
    let ada = identities
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["handle"] == "ada")
        .unwrap();
    assert_eq!(
        ada["emails"].as_array().unwrap().len(),
        1,
        "no duplicate email"
    );

    dedalo(repo.path())
        .args(["identity", "remove", "ada"])
        .assert()
        .success();
    dedalo(repo.path())
        .args(["identity", "remove", "ada"])
        .assert()
        .failure()
        .stderr(contains("no identity"));
}

/// The read-only commands are the ones people run in CI, often against a
/// checkout they cannot write to. None of them may leave `.dedalo` behind.
#[test]
fn read_only_commands_write_nothing() {
    let repo = project();
    let state = repo.path().join(".dedalo");
    assert!(!state.exists(), "the fixture starts clean");

    for args in [
        vec!["status"],
        vec!["scan"],
        vec!["contributors"],
        vec!["ledger"],
        vec!["identity", "list"],
        vec!["identity", "missing"],
        vec!["plan", "--amount", "1000"],
    ] {
        dedalo(repo.path()).args(&args).assert().success();
        assert!(
            !state.exists(),
            "`dedalo {}` created {}",
            args.join(" "),
            state.display()
        );
    }

    // `--save` is the point at which writing is asked for.
    dedalo(repo.path())
        .args(["plan", "--amount", "1000", "--save"])
        .assert()
        .success();
    assert!(state.exists(), "plan --save must persist the plan");
}

#[test]
fn every_command_emits_parseable_json() {
    let repo = project();
    // The Action runs each of these with --json and reads the result.
    for args in [
        vec!["status"],
        vec!["scan"],
        vec!["contributors"],
        vec!["ledger"],
        vec!["identity", "list"],
        vec!["identity", "missing"],
        vec!["plan", "--amount", "500"],
    ] {
        json_of(repo.path(), &args);
    }
}

#[test]
fn an_empty_range_is_an_empty_round_not_an_error() {
    let repo = project();
    let head = repo.head();
    let merges = json_of(repo.path(), &["scan", "--since", &head]);
    assert!(merges.as_array().unwrap().is_empty());

    dedalo(repo.path())
        .args(["contributors", "--since", &head])
        .assert()
        .success()
        .stdout(contains("No contributions"));
}

/// `dedalo scan | head` is an ordinary thing to type. Rust ignores SIGPIPE by
/// default, which used to turn it into a panic with a backtrace.
#[cfg(unix)]
#[test]
fn closing_the_pipe_early_does_not_panic() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command as StdCommand, Stdio};

    let repo = project();
    let exe = assert_cmd::cargo::cargo_bin("dedalo");

    let mut child = StdCommand::new(exe)
        .arg("-C")
        .arg(repo.path())
        .args(["--json", "scan"])
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("dedalo must start");

    // Read one line, then drop the pipe while the child is still writing.
    {
        let stdout = child.stdout.take().expect("piped");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
    }

    let output = child.wait_with_output().expect("dedalo must exit");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "a closed pipe must not panic:\n{stderr}"
    );
}

#[test]
fn help_and_version_work_without_a_project() {
    Command::cargo_bin("dedalo")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("dedalo"));

    // `-h` shows the short about, `--help` the long one. Both are the first
    // thing a new user reads, so both are worth pinning down.
    Command::cargo_bin("dedalo")
        .unwrap()
        .arg("-h")
        .assert()
        .success()
        .stdout(contains(
            "Turn code merges into sustainable open-source funding",
        ))
        .stdout(contains("settle"));

    Command::cargo_bin("dedalo")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Dedalo reads merge history from git"));
}

/// `identity link` is the only moment a human chooses an address, so it is the
/// only place worth saying what the address layer can and cannot catch.
///
/// Under EIP-55 that was a variable number of bits and the warning fired below
/// a threshold. A Solana address carries **no checksum at all**, so there is no
/// threshold and no quiet case: either it warns every time, or the address
/// layer has changed and this test should fail.
#[test]
fn linking_an_address_warns_that_there_is_no_checksum_to_rely_on() {
    let repo = project();

    let wallet = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU";
    dedalo(repo.path())
        .args([
            "identity",
            "link",
            "ada",
            wallet,
            "--email",
            "ada@example.com",
        ])
        .assert()
        .success()
        .stderr(contains("no checksum"));

    let linked = json_of(
        repo.path(),
        &[
            "identity",
            "link",
            "ada",
            wallet,
            "--email",
            "ada@example.com",
        ],
    );
    assert_eq!(linked["checksum_bits"], 0);
}

/// A share can only be claimed by whoever can sign for the address it names.
/// An off-curve address has no keypair, so linking one would create a
/// contributor who can never be paid — and the plan would not report it,
/// because as far as the plan is concerned they have a wallet.
#[test]
fn linking_an_address_nobody_can_sign_for_is_refused() {
    let repo = project();

    // Found rather than hardcoded: roughly half of all byte patterns are not
    // curve points, and which ones is not a thing to memorise.
    let mut raw = [0u8; 32];
    let off_curve = (0..=u8::MAX)
        .find_map(|byte| {
            raw[0] = byte;
            let candidate = dedalo::chain::wallet::Address::from_pubkey_bytes(raw);
            (!candidate.is_on_curve()).then(|| candidate.to_string())
        })
        .expect("some byte pattern is off the curve");

    dedalo(repo.path())
        .args([
            "identity",
            "link",
            "pda",
            &off_curve,
            "--email",
            "pda@example.com",
        ])
        .assert()
        .failure()
        .stderr(contains("not a wallet"));
}

/// The ledger is a hash chain, and `dedalo verify` is what makes that worth
/// anything: a third party with a clone can run it, and an entry edited after
/// the fact stops it.
#[test]
fn verify_accepts_an_intact_ledger_and_rejects_an_edited_one() {
    let repo = project();

    dedalo(repo.path())
        .args(["plan", "--amount", "1000", "--save"])
        .assert()
        .success();

    let report = json_of(repo.path(), &["verify"]);
    assert_eq!(report["ok"], true);
    assert_eq!(report["entries"], 1);
    let head = report["head"].as_str().unwrap().to_string();
    assert!(head.starts_with("dedc"), "{head}");

    // The stored entry, found the way the store lays it out: objects/<2>/<rest>.
    let (shard, rest) = head.split_at(2);
    let entry = repo
        .path()
        .join(".dedalo/objects")
        .join(shard)
        .join(format!("{rest}.json"));
    let raw = std::fs::read_to_string(&entry).unwrap();
    assert!(raw.contains("plan-created"), "{raw}");

    // Rewrite history: claim the round covered more merges than it did.
    std::fs::write(&entry, raw.replace("\"merges\": 2", "\"merges\": 9")).unwrap();

    dedalo(repo.path())
        .args(["verify"])
        .assert()
        .failure()
        .stderr(contains("changed after it was written"))
        .stderr(contains(&head));
}

/// A ledger written before the chain existed must not be read as empty:
/// every past round would look unpaid, and a retried job would pay it again.
#[test]
fn a_pre_chain_ledger_stops_the_cli_until_it_is_migrated() {
    let repo = project();
    let dir = repo.path().join(".dedalo");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("ledger.jsonl"),
        "{\"event\":\"settled\",\"at\":1,\"plan_id\":\"ded1f643ee0b221a82c5b7fce39c04d0d591\",\
         \"backend\":\"solana\",\"total\":\"5000\",\"dry_run\":false}\n",
    )
    .unwrap();

    dedalo(repo.path())
        .args(["ledger"])
        .assert()
        .failure()
        .stderr(contains("hash chain"))
        .stderr(contains("paid again"));

    let migrated = json_of(repo.path(), &["ledger", "--migrate"]);
    assert_eq!(migrated["migrated"], 1);

    // The old file is kept, not deleted, and the event is now in the chain.
    assert!(dir.join("ledger.jsonl.migrated").is_file());
    assert!(!dir.join("ledger.jsonl").exists());
    let entries = json_of(repo.path(), &["ledger"]);
    assert_eq!(entries.as_array().unwrap().len(), 1);
    assert_eq!(
        entries[0]["plan_id"],
        "ded1f643ee0b221a82c5b7fce39c04d0d591"
    );

    // And the settlement it recorded still counts, which is the whole point.
    let report = json_of(repo.path(), &["verify"]);
    assert_eq!(report["ok"], false, "the plan it names was never stored");
}

/// `dedalo propose` is the whole settlement story: Dedalo produces the
/// transactions and no signature. The test pins what a signer would paste.
#[test]
fn propose_emits_an_approval_then_a_deposit_and_signs_nothing() {
    let repo = project();

    // A round needs a cluster and a claim program to be proposed at all.
    let config_path = repo.path().join("dedalo.toml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    std::fs::write(
        &config_path,
        config.replacen(
            "backend = \"dry-run\"",
            "backend = \"solana\"\ncluster = \"devnet\"\n\
             program_id = \"MerkS3LaQBSvM5JZsvBaLZBBSMvMB5aTuLRHrvKAyDo\"",
            1,
        ),
    )
    .unwrap();

    let proposal = json_of(repo.path(), &["propose", "--amount", "1000"]);

    assert_eq!(proposal["instructions"].as_array().unwrap().len(), 2);
    assert_eq!(proposal["instructions"][0]["step"], 1);
    assert_eq!(proposal["instructions"][1]["step"], 2);
    assert_eq!(proposal["instructions"][0]["cluster"], "devnet");

    // The two instructions belong to two different programs, which is the
    // thing a signer most needs to see: an approval the token program will
    // execute, and a deposit the claim program will.
    assert_eq!(
        proposal["instructions"][0]["program_id"],
        "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
    );
    assert_eq!(
        proposal["instructions"][1]["program_id"],
        proposal["claim_program"]
    );

    // SPL Token dispatches Approve on a leading 4.
    let approve = proposal["instructions"][0]["data"].as_str().unwrap();
    let deposit = proposal["instructions"][1]["data"].as_str().unwrap();
    assert!(approve.starts_with("04"), "{approve}");

    // The root is thirty-two bytes, and it is what the deposit carries.
    let root = proposal["merkle_root"].as_str().unwrap();
    assert_eq!(root.len(), 2 + 64, "{root}");
    assert!(deposit.contains(root.trim_start_matches("0x")), "{deposit}");

    // Every account is either named or explained. A blank is something a
    // signer would have to invent.
    for step in proposal["instructions"].as_array().unwrap() {
        let accounts = step["accounts"].as_array().unwrap();
        assert!(!accounts.is_empty());
        for account in accounts {
            assert!(
                account["address"].is_string() || account["derivation"].is_string(),
                "an account is neither named nor derived: {account}"
            );
        }
    }

    // Nothing anywhere is a signature or a key.
    let raw = proposal.to_string();
    for forbidden in ["signature", "privateKey", "signer_env", "\"v\":", "\"r\":"] {
        assert!(!raw.contains(forbidden), "proposal mentions {forbidden}");
    }
}

/// The backend that used to promise a broadcast now says why there is none.
#[test]
fn settling_through_solana_explains_that_dedalo_holds_no_key() {
    let repo = project();
    let config_path = repo.path().join("dedalo.toml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    std::fs::write(
        &config_path,
        config.replacen(
            "backend = \"dry-run\"",
            "backend = \"solana\"\ncluster = \"devnet\"\n\
             program_id = \"MerkS3LaQBSvM5JZsvBaLZBBSMvMB5aTuLRHrvKAyDo\"",
            1,
        ),
    )
    .unwrap();

    dedalo(repo.path())
        .args(["settle", "--amount", "1000", "--execute"])
        .assert()
        .failure()
        .stderr(contains("holds no signing key"))
        .stderr(contains("dedalo propose"));
}

/// Completions and the man page must work with no repository anywhere near.
///
/// Every other command opens an engine first, which needs a `dedalo.toml`.
/// Somebody setting up their shell has not necessarily cloned anything yet, so
/// these two run before that lookup — and this test is what says so, by
/// running them from a directory that is not a project.
#[test]
fn completions_and_the_man_page_need_no_project() {
    let empty = tempfile::tempdir().unwrap();

    for (shell, marker) in [
        ("bash", "_dedalo()"),
        ("zsh", "#compdef dedalo"),
        ("fish", "complete -c dedalo"),
        ("powershell", "Register-ArgumentCompleter"),
        ("elvish", "edit:completion"),
    ] {
        Command::cargo_bin("dedalo")
            .unwrap()
            .current_dir(empty.path())
            .args(["completions", shell])
            .assert()
            .success()
            .stdout(contains(marker))
            // The flags that decide what happens to money are the reason this
            // is worth shipping at all.
            .stdout(contains("--execute"));
    }

    Command::cargo_bin("dedalo")
        .unwrap()
        .current_dir(empty.path())
        .arg("man")
        .assert()
        .success()
        .stdout(contains(".TH dedalo 1"))
        .stdout(contains("propose"));
}

/// Both are hidden from the command list, and `--help` says so at the bottom.
///
/// Hidden and undiscoverable are different things. If the hint ever falls out
/// of `--help`, the only way anybody learns these exist is by reading the
/// source.
#[test]
fn help_hides_the_generators_but_still_names_them() {
    Command::cargo_bin("dedalo")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains(
            "dedalo completions <bash|zsh|fish|powershell|elvish>",
        ))
        .stdout(contains("dedalo man"));
}
