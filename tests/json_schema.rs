//! Every command's real `--json` output, against the schema published beside
//! it.
//!
//! `schema/` is worth nothing unless it describes what the code actually
//! emits. A schema written once and left alone is a more confident lie than
//! prose, because a consumer generates a client from it.
//!
//! So these tests run the real commands against a real repository and validate
//! the bytes that come out. A field renamed in Rust and not in the schema fails
//! here.
//!
//! Every schema sets `additionalProperties: false`, which means **adding** a
//! field fails too, until the schema says so. That is deliberate: a field that
//! appears without being declared is one no consumer knows to expect and no
//! reviewer was asked about.

use std::path::Path;

use assert_cmd::Command;
use dedalo::testing::TempRepo;
use serde_json::Value;

/// Compile a schema from `schema/`, or explain which one is malformed.
fn schema_for(name: &str) -> jsonschema::Validator {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("schema")
        .join(format!("{name}.schema.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let document: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()));
    jsonschema::validator_for(&document)
        .unwrap_or_else(|e| panic!("{} is not a valid schema: {e}", path.display()))
}

/// Validate, and fail with every error rather than the first.
///
/// One renamed field usually breaks several rules at once, and being told all
/// of them is the difference between one fix and four rounds of guessing.
fn check(name: &str, instance: &Value) {
    let validator = schema_for(name);
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|error| format!("  at {}: {error}", error.instance_path()))
        .collect();

    assert!(
        errors.is_empty(),
        "`{name}` output does not match schema/{name}.schema.json:\n{}\n\n\
         Output was:\n{}",
        errors.join("\n"),
        serde_json::to_string_pretty(instance).unwrap_or_default()
    );
}

fn dedalo(repo: &Path) -> Command {
    let mut command = Command::cargo_bin("dedalo").unwrap();
    command.current_dir(repo);
    command
}

/// A repository with real merges, a config, and one linked contributor.
///
/// One linked and one not, on purpose: a plan where everybody is payable never
/// populates `unresolved`, and a plan where nobody is never populates `items`.
/// The schemas have to describe both halves.
fn project() -> TempRepo {
    let repo = TempRepo::new("schema");
    repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 30);
    repo.merge_feature("feature-b", ("Bea", "bea@example.com"), 10);

    dedalo(repo.path())
        .args(["init", "--name", "demo", "--open-collective", "demo"])
        .assert()
        .success();

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

    // Ada is payable; Bea is not, so `unresolved` has something in it.
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

    repo
}

fn json_from(repo: &Path, args: &[&str]) -> Value {
    let output = dedalo(repo)
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output)
        .unwrap_or_else(|e| panic!("`dedalo {}` did not emit JSON: {e}", args.join(" ")))
}

#[test]
fn a_plan_matches_its_schema() {
    let repo = project();
    let plan = json_from(repo.path(), &["plan", "--amount", "1000", "--json"]);

    // The fixture is only useful if it exercises both halves.
    assert!(
        !plan["items"].as_array().unwrap().is_empty(),
        "the fixture paid nobody, so `items` proves nothing"
    );
    assert!(
        !plan["unresolved"].as_array().unwrap().is_empty(),
        "the fixture resolved everybody, so `unresolved` proves nothing"
    );

    check("plan", &plan);
}

#[test]
fn attribution_matches_its_schema() {
    let repo = project();
    let attribution = json_from(repo.path(), &["contributors", "--json"]);
    assert!(!attribution["contributions"].as_array().unwrap().is_empty());
    check("attribution", &attribution);
}

#[test]
fn status_matches_its_schema() {
    let repo = project();
    check("status", &json_from(repo.path(), &["status", "--json"]));
}

#[test]
fn verify_matches_its_schema() {
    let repo = project();
    check("verify", &json_from(repo.path(), &["verify", "--json"]));
}

/// A proposal is what a signer reads before moving money, so its shape is the
/// one that matters most and the one hardest to fake.
///
/// `propose` needs a cluster and a claim program in the config, because a
/// proposal that did not say which network it was for would be a thing nobody
/// could safely sign. So the fixture supplies both — the program id is an
/// obvious placeholder, since the real claim program does not exist.
#[test]
fn a_proposal_matches_its_schema() {
    let repo = project();
    let config_path = repo.path().join("dedalo.toml");
    // The template already has a `[settlement]` section, so this replaces its
    // body rather than appending a second one.
    let config = std::fs::read_to_string(&config_path).unwrap();
    let (before, _) = config
        .split_once("[settlement]")
        .expect("the template has a [settlement] section");
    let config = format!(
        "{before}[settlement]\nbackend = \"solana\"\ncluster = \"devnet\"\n\
         program_id = \"MerkS3LaQBSvM5JZsvBaLZBBSMvMB5aTuLRHrvKAyDo\"\n"
    );
    std::fs::write(&config_path, config).unwrap();

    let proposal = json_from(repo.path(), &["propose", "--amount", "1000", "--json"]);
    assert!(
        !proposal["instructions"].as_array().unwrap().is_empty(),
        "a proposal with no instructions asks a signer to do nothing"
    );
    check("proposal", &proposal);
}

/// The schemas are themselves well-formed, and say which version they are.
///
/// A schema with a broken `$ref` or a missing `$id` compiles into something
/// that validates nothing, and every test above would pass against it.
#[test]
fn every_schema_compiles_and_declares_its_version() {
    for name in ["plan", "attribution", "status", "verify", "proposal"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("schema")
            .join(format!("{name}.schema.json"));
        let document: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));

        let id = document["$id"]
            .as_str()
            .unwrap_or_else(|| panic!("{name} has no $id"));
        assert!(
            id.contains("/v1/"),
            "{name}: the version lives in the $id, and {id} has none"
        );
        assert!(
            id.ends_with(&format!("{name}.schema.json")),
            "{name}: $id is {id}"
        );

        // Refuse a schema that would accept anything.
        assert_eq!(
            document["additionalProperties"],
            Value::Bool(false),
            "{name}: without additionalProperties:false a new field is invisible"
        );

        let _ = schema_for(name);
    }
}

/// An amount is a decimal string everywhere it appears, and the schema says so.
///
/// This is the single most important thing the schema communicates: a consumer
/// that parses an amount as a JSON number has a bug that will not show up until
/// the amounts get large, and by then it is a payout.
#[test]
fn every_amount_in_a_plan_is_a_string_of_digits() {
    let repo = project();
    let plan = json_from(repo.path(), &["plan", "--amount", "1000", "--json"]);

    let mut checked = 0;
    let mut assert_amount = |value: &Value, what: &str| {
        let text = value
            .as_str()
            .unwrap_or_else(|| panic!("{what} is {value}, not a string"));
        assert!(
            text.bytes().all(|b| b.is_ascii_digit()),
            "{what} is {text:?}, which is not a count of base units"
        );
        checked += 1;
    };

    for field in ["gross", "protocol", "treasury", "contributors"] {
        assert_amount(&plan["split"][field], field);
    }
    assert_amount(&plan["undistributed"], "undistributed");
    for item in plan["items"].as_array().unwrap() {
        assert_amount(&item["amount"], "an item amount");
    }

    assert!(checked >= 6, "only {checked} amounts were checked");
}
