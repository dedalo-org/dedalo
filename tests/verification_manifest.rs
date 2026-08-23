//! The gate that makes `verification.toml` mean something.
//!
//! A manifest nobody checks is a wish list. This test fails the build when:
//!
//! - a module under `src/` is not accounted for, so a new one cannot be added
//!   without someone deciding what verifies it;
//! - the manifest names a module that no longer exists;
//! - a declared harness is not in the test suite, so a proof cannot be claimed
//!   after the test that made it was deleted;
//! - the money arithmetic in a module no longer matches the recorded count, so
//!   arithmetic cannot appear in a module without being noticed;
//! - a module claiming exemption does arithmetic, or builds an address —
//!   exemption means "decides neither how much nor where", and that is checked
//!   rather than trusted.
//!
//! What it deliberately does not do is judge whether the *method* is strong
//! enough. That is a human call, recorded in the manifest's `notes`, and this
//! test's job is to make sure the call was made and stays honest.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Manifest {
    modules: BTreeMap<String, Entry>,
    contracts: BTreeMap<String, ContractEntry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    method: String,
    arithmetic_sites: usize,
    #[serde(default)]
    harnesses: Vec<String>,
    #[serde(default)]
    reason: Option<String>,
    /// Why the arithmetic this module does is not about money.
    ///
    /// `saturating_sub` on a slice length is not a payout. The pattern match
    /// cannot tell, so a human says so here — and the count above still has to
    /// match, so the statement is re-examined whenever the code moves.
    #[serde(default)]
    arithmetic_is_not_money: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContractEntry {
    method: String,
    target: String,
    max_compressed_kib: usize,
}

const METHODS: [&str; 5] = ["exhaustive", "property", "tests", "proofs", "exempt"];

/// Anything that adds, subtracts, multiplies or converts money.
///
/// Deliberately textual. A type-aware count would be more precise and would
/// need the compiler; this needs to run as an ordinary test and to be
/// reproducible by anyone reading it with `grep`.
const ARITHMETIC: [&str; 11] = [
    "checked_add",
    "checked_sub",
    "checked_mul",
    "split_by_weights",
    ".bps(",
    "base_units()",
    "Amount::",
    // Attribution scores in milli-points, not `Amount`, and they decide the
    // weights every payout is split by. Counting only `Amount` would let the
    // module that chooses who is owed what look arithmetic-free.
    "saturating_add",
    "saturating_mul",
    "saturating_sub",
    "wrapping_",
];

/// Building a destination. A module that does this decides where funds land,
/// so it may not claim exemption even with no arithmetic.
const ADDRESSES: [&str; 2] = ["Address::", "wallet::"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every module under `src/`, named the way the manifest names them.
fn modules_on_disk() -> BTreeMap<String, PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("src/ is readable") {
            let path = entry.expect("a readable entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    let src = root().join("src");
    let mut files = Vec::new();
    walk(&src, &mut files);

    files
        .into_iter()
        .filter_map(|path| {
            let relative = path.strip_prefix(&src).expect("under src/").to_owned();
            let name = relative
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            let name = name.strip_suffix("/mod").unwrap_or(&name).to_string();
            // The crate root and the binary shim are not modules to verify.
            if name == "lib" || name == "main" {
                return None;
            }
            // `src/chain/contract` is a separate crate with its own manifest:
            // it compiles for another target and is not part of this module
            // tree. It is accounted for under [contracts] instead.
            if name.starts_with("chain/contract/") {
                return None;
            }
            Some((name, path))
        })
        .collect()
}

fn manifest() -> Manifest {
    let path = root().join("verification.toml");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    toml::from_str(&raw).unwrap_or_else(|e| panic!("{} is not valid: {e}", path.display()))
}

/// Count occurrences of the money patterns, ignoring the module's own tests:
/// a `#[cfg(test)]` block is not shipped and its arithmetic decides nothing.
fn shipped_source(path: &Path) -> String {
    // Normalised, because git hands out CRLF on Windows and the marker below
    // would never match — every module's tests would then be counted as
    // shipped arithmetic, and the manifest would be wrong on one platform only.
    let source = std::fs::read_to_string(path)
        .expect("a readable module")
        .replace("\r\n", "\n");
    match source.find("\n#[cfg(test)]\nmod tests {") {
        Some(index) => source[..index].to_string(),
        None => source,
    }
}

fn arithmetic_sites(path: &Path) -> usize {
    let body = shipped_source(path);
    body.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//") && ARITHMETIC.iter().any(|p| line.contains(p))
        })
        .count()
}

fn builds_addresses(path: &Path) -> bool {
    let body = shipped_source(path);
    body.lines().any(|line| {
        let trimmed = line.trim_start();
        !trimmed.starts_with("//")
            && !trimmed.starts_with("///")
            && ADDRESSES.iter().any(|p| line.contains(p))
    })
}

/// Every test function, named the way the manifest names it.
///
/// Two shapes, because there are two kinds of test and the manifest should say
/// which it means:
///
/// - `<file stem>::<fn>` for an integration test under `tests/`, which links
///   the crate from outside like any other consumer;
/// - `<module path>::<fn>` for a unit test under `src/`, which sees the
///   module's private surface.
fn harnesses_on_disk() -> BTreeSet<String> {
    fn functions_in(source: &str, prefix: &str, found: &mut BTreeSet<String>) {
        for line in source.replace("\r\n", "\n").lines() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("fn ")
                && let Some(name) = rest.split(['(', '<']).next()
            {
                found.insert(format!("{prefix}::{name}"));
            }
        }
    }

    let mut found = BTreeSet::new();

    for entry in std::fs::read_dir(root().join("tests")).expect("tests/ is readable") {
        let path = entry.expect("a readable entry").path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let stem = path
            .file_stem()
            .expect("a file stem")
            .to_string_lossy()
            .to_string();
        let source = std::fs::read_to_string(&path).expect("a readable test file");
        functions_in(&source, &stem, &mut found);
    }

    for (module, path) in modules_on_disk() {
        let source = std::fs::read_to_string(&path).expect("a readable module");
        functions_in(&source, &module.replace('/', "::"), &mut found);
    }

    found
}

#[test]
fn every_module_is_accounted_for() {
    let manifest = manifest();
    let on_disk = modules_on_disk();

    let declared: BTreeSet<&String> = manifest.modules.keys().collect();
    let present: BTreeSet<&String> = on_disk.keys().collect();

    let undeclared: Vec<_> = present.difference(&declared).collect();
    assert!(
        undeclared.is_empty(),
        "these modules are not in verification.toml — say how each one is \
         verified, or why it needs no verification: {undeclared:?}"
    );

    let stale: Vec<_> = declared.difference(&present).collect();
    assert!(
        stale.is_empty(),
        "verification.toml names modules that no longer exist: {stale:?}"
    );
}

#[test]
fn every_declared_method_is_one_this_project_means_something_by() {
    let manifest = manifest();
    for (name, entry) in &manifest.modules {
        assert!(
            METHODS.contains(&entry.method.as_str()),
            "{name}: `{}` is not a method; expected one of {METHODS:?}",
            entry.method
        );
        if entry.method == "exempt" {
            assert!(
                entry.reason.is_some(),
                "{name}: an exemption without a reason is a gap nobody can review"
            );
        } else if entry.method != "tests" && entry.method != "proofs" {
            assert!(
                !entry.harnesses.is_empty(),
                "{name}: `{}` must name the harnesses that carry it",
                entry.method
            );
        }
    }

    for (path, entry) in &manifest.contracts {
        // A deployable is a binding around rules verified elsewhere, so what
        // is checked here is that it exists, names its target, and declares
        // the size limit `ws-check` measures it against. A contract that
        // cannot be deployed is not a contract.
        assert_eq!(
            entry.method, "binding",
            "{path}: a deployable binds rules, it does not hold them"
        );
        assert!(
            !entry.target.is_empty(),
            "{path}: name the target it compiles for"
        );
        assert!(
            entry.max_compressed_kib > 0,
            "{path}: declare the size limit"
        );
        assert!(
            root().join(path).join("Cargo.toml").is_file(),
            "{path}: declared but has no manifest"
        );
    }
}

#[test]
fn every_declared_harness_exists() {
    let manifest = manifest();
    let found = harnesses_on_disk();
    let mut missing = Vec::new();
    for (name, entry) in &manifest.modules {
        for harness in &entry.harnesses {
            if !found.contains(harness) {
                missing.push(format!("{name} claims {harness}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "a module claims a proof whose test does not exist — deleting the test \
         must not leave the claim behind: {missing:#?}"
    );
}

/// The ratchet. Arithmetic cannot appear in a module without the count moving,
/// and the count cannot move without this failing and someone looking.
#[test]
fn the_money_arithmetic_in_each_module_is_what_the_manifest_records() {
    let manifest = manifest();
    let on_disk = modules_on_disk();
    let mut drifted = Vec::new();

    for (name, entry) in &manifest.modules {
        let Some(path) = on_disk.get(name) else {
            continue; // reported by every_module_is_accounted_for
        };
        let actual = arithmetic_sites(path);
        if actual != entry.arithmetic_sites {
            drifted.push(format!(
                "{name}: manifest says {}, source has {actual}",
                entry.arithmetic_sites
            ));
        }
    }

    assert!(
        drifted.is_empty(),
        "money arithmetic moved. Update verification.toml, and while you are \
         there decide whether the module's method still covers it:\n{}",
        drifted.join("\n")
    );
}

/// Exemption is a claim about what a module does, so it is checked.
#[test]
fn nothing_exempt_decides_how_much_or_where() {
    let manifest = manifest();
    let on_disk = modules_on_disk();
    let mut wrong = Vec::new();

    for (name, entry) in &manifest.modules {
        if entry.method != "exempt" {
            continue;
        }
        let Some(path) = on_disk.get(name) else {
            continue;
        };

        let sites = arithmetic_sites(path);
        if sites != 0 && entry.arithmetic_is_not_money.is_none() {
            wrong.push(format!(
                "{name} is exempt but does arithmetic in {sites} place(s); either \
                 give it a method that covers them, or record \
                 `arithmetic_is_not_money` saying what they are"
            ));
        }
        if builds_addresses(path) {
            wrong.push(format!("{name} is exempt but builds an address"));
        }
    }

    assert!(
        wrong.is_empty(),
        "an exemption stopped being true. Reclassify these and give them a \
         method that covers what they now do:\n{}",
        wrong.join("\n")
    );
}
