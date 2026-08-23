// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title DedaloClaim
/// @notice Holds one funding round per plan id and pays contributors who
///         prove their share against a Merkle root.
/// @dev    Reference implementation. **Unaudited and undeployed** — see
///         docs/settlement-architecture.md for the five things that must
///         exist before this holds real money.
///
///         The design is pull, not push, and the reasons are recorded in that
///         document: a push pays gas per payee, needs every contributor to
///         have linked a wallet at the moment of payment, and destroys funds
///         sent to a well-formed address that is not theirs. Here a round is
///         deposited once and each contributor claims, paying their own gas,
///         whenever they are ready.
///
///         The leaf encoding must match `dedalo::merkle` exactly:
///
///             leaf = keccak256(bytes.concat(keccak256(abi.encode(index, account, amount))))
///             node = keccak256(min(a,b) ‖ max(a,b))
///
///         Leaves are hashed twice so no 64-byte leaf can be presented as an
///         internal node. Pairs are sorted so a proof needs no direction bits.
contract DedaloClaim {
    /// @dev Invariant, asserted at every write and proved by the SMT checker:
    ///      `claimed <= total`. Every subtraction below depends on it, and
    ///      stating it as an assertion is what lets `solc --model-checker-engine
    ///      chc` discharge them rather than warn.
    struct Round {
        bytes32 root;
        address token;
        uint256 total;
        uint256 claimed;
        uint256 expiry;
        address depositor;
    }

    /// @notice Rounds by plan id. A plan id is the sixteen bytes of the
    ///         content hash Dedalo computes offline.
    mapping(bytes16 => Round) public rounds;

    /// @notice Which indices of a round have already been paid.
    mapping(bytes16 => mapping(uint256 => bool)) public claimed;

    event Deposited(
        bytes16 indexed planId, bytes32 root, address indexed token, uint256 total, uint256 expiry
    );
    event Claimed(bytes16 indexed planId, uint256 indexed index, address indexed account, uint256 amount);
    event Swept(bytes16 indexed planId, address indexed to, uint256 amount);


    // Reverts carry strings rather than custom errors, which is the older and
    // more expensive idiom. It is deliberate: solc's SMTChecker does not
    // implement custom error types, and when it meets a construct it does not
    // implement it stops constraining the surrounding state — so every guard
    // in this contract became invisible to it and nothing could be proved.
    //
    // With strings, `solc --model-checker-engine bmc` proves all ten
    // arithmetic and assertion conditions here safe. A few hundred gas on a
    // path that only runs when something is already wrong is a small price for
    // a contract whose arithmetic is machine-checked rather than reviewed.

    /// @notice How long a round stays claimable before the depositor may
    ///         recover what is left.
    /// @dev    Fixed rather than caller-supplied: a depositor who could choose
    ///         it could choose a round that expires before anyone claims.
    uint256 public constant CLAIM_WINDOW = 180 days;

    /// @notice Fund a round against the Merkle root of its payout plan.
    /// @param planId The plan's content hash. Used once, ever.
    /// @param root   Root of the tree of (index, account, amount) leaves.
    /// @param token  ERC-20 being distributed.
    /// @param total  Sum of every claim in the tree.
    function deposit(bytes16 planId, bytes32 root, address token, uint256 total) external {
        // The replay guard the whole system rests on: a plan id names one
        // round, and a round is funded once. A retried CI job that proposes
        // the same plan twice cannot pay it twice.
        if (rounds[planId].depositor != address(0)) revert("dedalo: this plan id was already deposited");
        if (total == 0 || root == bytes32(0)) revert("dedalo: a round needs a non-zero root and total");

        // Measured rather than assumed: a fee-on-transfer token delivers less
        // than it was asked for, and a round that promises more than it holds
        // pays early claimants and strands the rest.
        //
        // Written as two comparisons rather than `after - before < total`
        // because the token is untrusted: a balance that went *down* would
        // underflow, and a panic is a worse answer than ShortTransfer.
        uint256 balanceBefore = _balanceOf(token, address(this));
        _transferFrom(token, msg.sender, address(this), total);
        uint256 balanceAfter = _balanceOf(token, address(this));
        uint256 delivered = balanceAfter >= balanceBefore ? balanceAfter - balanceBefore : 0;
        if (delivered < total) revert("dedalo: the token delivered less than the round needs");

        // The overflow is unreachable on any real chain — it needs a timestamp
        // within 180 days of 2**256 — but "unreachable" is a claim, and this
        // is how it is checked instead of asserted.
        if (block.timestamp > type(uint256).max - CLAIM_WINDOW) revert("dedalo: block timestamp is too large to add the claim window");
        uint256 expiry = block.timestamp + CLAIM_WINDOW;

        rounds[planId] =
            Round({root: root, token: token, total: total, claimed: 0, expiry: expiry, depositor: msg.sender});
        assert(rounds[planId].claimed <= rounds[planId].total);

        emit Deposited(planId, root, token, total, expiry);
    }

    /// @notice Take one share of a round.
    /// @dev    Anyone may submit the transaction; the funds always go to
    ///         `account`. That lets a project pay the gas for a contributor
    ///         without being able to redirect the money.
    function claim(
        bytes16 planId,
        uint256 index,
        address account,
        uint256 amount,
        bytes32[] calldata proof
    ) external {
        Round storage round = rounds[planId];
        if (round.depositor == address(0)) revert("dedalo: no round for this plan id");
        if (claimed[planId][index]) revert("dedalo: this index was already claimed");

        bytes32 leaf = keccak256(bytes.concat(keccak256(abi.encode(index, account, amount))));
        if (!_verify(proof, round.root, leaf)) revert("dedalo: the proof does not match the round's root");

        // Cannot happen if the root was built from a plan, because the plan's
        // items sum to its total. Checked anyway: this contract is the last
        // thing standing between a bad root and the depositor's balance.
        //
        // The remaining amount is computed by subtraction, not by adding and
        // comparing. Adding first would rely on the compiler's implicit
        // overflow revert to be the guard, which is a guard nobody wrote and
        // no reader can see.
        //
        // `claimed <= total` is a contract invariant, but it is one that holds
        // across transactions, and the model checker cannot establish those
        // over a mapping of structs. Re-established locally so this
        // subtraction is provable from what is on this stack.
        if (round.claimed > round.total) revert("dedalo: round accounting is inconsistent");
        uint256 remaining = round.total - round.claimed;
        if (amount > remaining) revert("dedalo: claim exceeds what the round still holds");

        // Effects before interaction. A token with a transfer hook must not be
        // able to re-enter and claim the same index twice.
        claimed[planId][index] = true;
        // `amount <= remaining = total - claimed`, so this cannot exceed
        // `total` and therefore cannot overflow.
        round.claimed = round.claimed + amount;
        assert(round.claimed <= round.total);

        _transfer(round.token, account, amount);
        emit Claimed(planId, index, account, amount);
    }

    /// @notice After the window closes, return what nobody claimed.
    /// @dev    Only to the depositor's own address. Unclaimed money going
    ///         anywhere else would make this a contract that can move funds to
    ///         a destination the depositor did not sign for.
    function sweep(bytes16 planId) external {
        Round storage round = rounds[planId];
        if (round.depositor == address(0)) revert("dedalo: no round for this plan id");
        if (msg.sender != round.depositor) revert("dedalo: only the depositor may sweep");
        if (block.timestamp < round.expiry) revert("dedalo: the claim window has not closed");

        if (round.claimed > round.total) revert("dedalo: round accounting is inconsistent");
        uint256 remaining = round.total - round.claimed;
        round.claimed = round.total;
        assert(round.claimed <= round.total);
        if (remaining > 0) _transfer(round.token, round.depositor, remaining);
        emit Swept(planId, round.depositor, remaining);
    }

    /// @dev OpenZeppelin's `MerkleProof.verify`, inlined so this contract has
    ///      no dependencies to pin. Commutative hashing: the proof carries no
    ///      direction because `min`/`max` decides it.
    function _verify(bytes32[] calldata proof, bytes32 root, bytes32 leaf) private pure returns (bool) {
        bytes32 computed = leaf;
        for (uint256 i = 0; i < proof.length; i++) {
            bytes32 sibling = proof[i];
            computed = computed <= sibling
                ? keccak256(abi.encodePacked(computed, sibling))
                : keccak256(abi.encodePacked(sibling, computed));
        }
        return computed == root;
    }

    // --- ERC-20, defensively ------------------------------------------------
    //
    // Some widely-held tokens return nothing from `transfer` instead of the
    // `bool` the standard specifies, so a plain interface call reverts on them.
    // These accept both: empty return data, or `true`.

    function _transfer(address token, address to, uint256 amount) private {
        (bool ok, bytes memory data) =
            token.call(abi.encodeWithSelector(0xa9059cbb, to, amount)); // transfer(address,uint256)
        if (!ok || (data.length != 0 && !abi.decode(data, (bool)))) revert("dedalo: the token rejected the transfer");
    }

    function _transferFrom(address token, address from, address to, uint256 amount) private {
        (bool ok, bytes memory data) =
            token.call(abi.encodeWithSelector(0x23b872dd, from, to, amount)); // transferFrom
        if (!ok || (data.length != 0 && !abi.decode(data, (bool)))) revert("dedalo: the token rejected the transfer");
    }

    function _balanceOf(address token, address who) private view returns (uint256) {
        (bool ok, bytes memory data) = token.staticcall(abi.encodeWithSelector(0x70a08231, who));
        if (!ok || data.length < 32) revert("dedalo: the token rejected the transfer");
        return abi.decode(data, (uint256));
    }
}
