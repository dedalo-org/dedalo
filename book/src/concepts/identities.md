# Identities and wallets

Attribution scores git emails. A transfer needs an address. The mapping between
them is the only part of a round a human types in, which makes it the only part
that can be wrong in a way arithmetic cannot catch.

## The shape

```toml
[[identities]]
handle = "ada"
wallet = "0xAdA0000000000000000000000000000000000000"
emails = ["ada@example.com", "ada@work.example"]
```

One handle, one wallet, many emails. Managed from the command line so the file
stays valid:

```bash
dedalo identity link ada 0xAdA… --email ada@example.com --email ada@work.example
dedalo identity list
dedalo identity missing
dedalo identity remove ada
```

## One wallet, one transfer

A contributor who commits from a laptop, a work machine and the GitHub web
editor has three emails in the history. Attribution scores all three. Without
merging, the plan would contain three items paying the same address — three
transfers, three times the gas, and a payout table that looks like three people
where there is one.

So contributors are merged into a single item **before** the plan is finalised,
keyed on the wallet. Addresses are compared **exactly**, because base58 has one
encoding per value: an account has a single written form, so two strings that
differ at all are two different accounts.

That is the opposite of what the previous chain family needed, and getting it
backwards is a real hazard rather than a hypothetical one. EIP-55 hid a checksum
in an address's capitalisation, so comparison there had to fold case — and
carrying that habit across would merge two unrelated Solana accounts into one
payee, paying one person twice and the other never.

Both directions are tests, not remarks. `tests/adversarial.rs` asks whether one
account listed three times can be paid three times, and whether two accounts
differing only in case can be merged.

## Nobody is silently dropped

A contributor whose email matches no identity does not vanish. They appear in
the plan's `unresolved` list, with a reason:

```json
{
  "unresolved": [
    { "email": "cy@example.com", "reason": "no identity links this email" }
  ],
  "undistributed": "197710000"
}
```

and their share is counted in `undistributed`, so the plan still balances
exactly. Two ways to resolve it, and the choice is real:

- **Link them and re-plan.** Requires reaching the person. The round then pays
  them directly.
- **Fund the round anyway.** Under the [pull model](../operating/multisig.md)
  their share sits in the round against the Merkle root until they link a
  wallet and claim it. Nothing is lost by waiting.

## There is no checksum

`identity link` validates an address before writing it down, and reports how
much that validation is worth. On Solana the honest answer is: **not much.**

A Solana address is thirty-two bytes written in base58. Every thirty-two byte
value is a valid public key, so there is nothing for a mistyped address to fail
against. [`Address::checksum_bits`][bits] returns **zero**, and it is not a
placeholder.

This is a loss, and it is worth naming what was lost. The previous chain family
used [EIP-55][eip55], which hid a checksum in the capitalisation of an address's
hex letters — around fifteen bits on a typical address, so most typos were
caught.

| | EIP-55 | Solana |
| --- | --- | --- |
| Bits of checksum | ~15 typical, 7 unlucky | **0, always** |
| A single-character typo | usually rejected | usually accepted |

Two things blunt it, and neither is a fix:

- **Length.** base58 is dense enough that many slips change the decoded length
  and are rejected outright. `most_single_character_slips_produce_another_valid_address`
  in `tests/adversarial.rs` measures how many are not, and asserts the number is
  bad, so this page cannot quietly become optimistic.
- **The curve.** Roughly half of all thirty-two byte values are not points on
  ed25519, and a wallet must be one — see below.

So `identity link` warns every time, rather than below a threshold:

```console
warning: a Solana address carries no checksum. Every thirty-two byte value is
         a valid key, so a mistyped address that still decodes is accepted and
         belongs to nobody. Compare it against the wallet, character by
         character, before a round runs.
```

What actually protects a contributor is not the address layer but
[the pull model](./settlement.md): a round is *claimed*, not sent. A share
nobody can claim stays in the round until it expires, instead of landing
somewhere unrecoverable.

## A wallet must be something somebody can sign for

`identity link` refuses an address that is **off the ed25519 curve**.

An ordinary wallet is a keypair's public key, which is a point on the curve. A
program-derived address deliberately is not — that is what makes it unforgeable
by a keypair. So is every associated token account.

Linking one as a contributor's wallet would create somebody who can never claim
their share, and the plan would not report it: as far as the plan is concerned
they have a wallet. So this is refused rather than warned about.

> **Link the wallet, not the token account.** The token account is derived from
> the wallet and the mint, and deriving it is the claim program's job. A
> treasury or a multisig vault *is* legitimately off-curve — a Squads vault is
> program-derived — which is why the rule applies to contributors and not to
> `[wallets]`.

> **Careful** — for a wallet that will receive real money, confirm the address
> out of band: read the first and last six characters back to the person over a
> channel that is not the one it arrived on. On this chain there is no checksum
> to catch a typing mistake at all, and nothing anywhere catches an address that
> was substituted in transit.

## Handles

The handle is a label. It appears in the payout table and in `--json`, and it
is usually a GitHub username — but nothing checks that, and nothing resolves it
against any service. Dedalo does not call GitHub to find out who anybody is; it
reads the repository and the config, and that is the whole of it.

The handle is **not** part of the [plan id](plans.md#the-id). Renaming `ada` to
`ada-lovelace` leaves the id unchanged, because the same wallet still receives
the same amount. The wallet is what a plan commits to; the handle is how it is
read.

[eip55]: https://eips.ethereum.org/EIPS/eip-55
[bits]: https://docs.rs/dedalo/latest/dedalo/chain/wallet/struct.Address.html#method.checksum_bits
