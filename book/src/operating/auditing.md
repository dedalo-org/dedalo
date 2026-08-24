# Auditing a project

You have found a project that says it pays contributors with Dedalo. This
chapter is how to check that from the outside, without asking the maintainer
for anything.

Everything below runs on a clone. None of it needs a key, a token, an API, or
the maintainer's cooperation — which is the property that makes the claim worth
anything.

## 1. Clone and verify the chain

```bash
git clone --filter=blob:none https://github.com/some/project
cd project
dedalo verify
```

```console
head dedc6ddbbef5415e6dcbf805b60affd83c49
ok 4 entries hash to their recorded ids
ok 2 settled plans present and self-consistent
```

Each ledger entry names its parent and hashes over it, so an entry edited after
the fact breaks every id after it and `HEAD` stops matching. A pass means the
record you are reading is the record that was written.

A failure means one of three things: an entry was edited, an object is missing
from the clone, or `.dedalo/` was partly committed. All three are worth asking
about.

## 2. Recompute a round

```bash
dedalo plan --amount 1000 --since af3141b5 --json | jq -r .id
```

Compare the id against the one in the ledger entry. If they match, the
published round is exactly what this history and this config produce. If they
do not, either the config changed after the round or the numbers did not come
from here.

> **Note** — use the same `--since` and the same amount the round used. Both
> are recorded in the ledger entry, so this is a lookup rather than a guess.

## 3. Read the policy

`dedalo.toml` is committed, and it is the whole policy. Four things to look at:

| Read | Ask |
| --- | --- |
| `[fees]` | Where does the money that is not paid to contributors go? |
| `[wallets]` | Are these real, and does anybody say who controls them? |
| `[attribution]` | Does the scoring match what the project says it rewards? |
| `[git] ignore_emails` | Is anybody excluded who should not be? |

`git log -p dedalo.toml` shows every time the policy changed and who approved
it. A funding policy that changes the round before a round is not necessarily
wrong — but it is a thing to see rather than not see.

## 4. Look at who is not being paid

```bash
dedalo plan --amount 1000 --json | jq '.unresolved'
```

`unresolved` lists contributors who earned a share and have no wallet on file.
A long list on a project that has run several rounds means people are earning
and not being reached.

## 5. Check the money adds up

```bash
dedalo plan --amount 1000 --json |
  jq '[( .items[].amount | tonumber ), (.undistributed | tonumber)] | add == (.gross|tonumber)'
```

The code guarantees this and property tests hold it down. Checking it once
yourself is how you find out that you understand the document rather than
trusting the sentence that describes it.

## What auditing this does not tell you

Being clear about the limits is the point of the exercise:

- **It does not tell you the money arrived.** The ledger records what was
  planned and settled from Dedalo's side. Whether a transaction was executed by
  the multisig, and whether contributors claimed, is a question for the chain.
- **It does not tell you the scoring is fair.** It tells you the scoring is
  what the config says. Whether the config is fair is a judgement, and it is
  the project's to make and yours to disagree with.
- **It does not audit the contract.** The vault is unaudited and undeployed.
  See [Funding from a multisig](multisig.md#before-real-funds-move).
- **It does not verify identities.** That a handle maps to a wallet is an
  assertion the maintainer made. Nothing checks that `ada` is the Ada you think.

## For funders

If you are considering funding a project through Dedalo, the four checks above
take about ten minutes and answer the question "does what they publish match
what their repository produces". That is a narrower question than "is this
project worth funding", and it is the one that can be answered mechanically.

The rest — whether the work is good, whether the split is fair, whether the
maintainers are who they say — is the same judgement funding anything requires.
Dedalo's contribution is removing the part that used to require trust and can
instead be arithmetic.
