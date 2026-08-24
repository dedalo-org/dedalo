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
keyed on the wallet. Addresses are compared **case-insensitively**, because
EIP-55 checksumming means the same account has two valid spellings and a
case-sensitive comparison would treat them as two payees.

That last sentence is a test, not a remark: `tests/adversarial.rs` asks
specifically whether one account spelled two ways can be paid twice.

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

## How strong is the checksum

`identity link` validates an address before writing it down, and reports how
much that validation is worth.

An Ethereum address is 40 hex characters. [EIP-55][eip55] hides a checksum in
the **capitalisation of the hex letters** — the characters in `a-f`. Digits
carry no case, so they carry no checksum. An address with many letters is well
protected; an address that happens to be mostly digits is barely protected at
all.

| Letters in the address | Bits of checksum | A typo survives with probability |
| --- | --- | --- |
| 20 (typical) | 20 | ~1 in 1,000,000 |
| 15 (average) | 15 | ~1 in 32,000 |
| 7 (unlucky) | 7 | ~1 in 128 |
| 0 (all digits) | 0 | always |

[`Address::checksum_bits`][bits] returns the number, and `identity link` warns
when it is low:

```console
warning: EIP-55 checksum carries 7 bits for this address
         a typo has roughly a 1-in-128 chance of surviving it
         confirm the address with ada through a second channel
```

This is not the tool covering itself. The residual risk genuinely belongs to
whoever pasted the address — no validator can recover a checksum that the
encoding never carried — and saying so is more useful than a green tick that
means less than it looks like it means.

> **Careful** — for a wallet that will receive real money, confirm the address
> out of band: read the first and last six characters back to the person over a
> channel that is not the one it arrived on. The checksum catches typing
> mistakes. It does not catch an address that was substituted in transit.

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
