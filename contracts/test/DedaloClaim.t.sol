// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {DedaloClaim} from "../src/DedaloClaim.sol";

/// The one cheatcode these tests need, declared here so the suite pulls in no
/// library at all.
interface Vm {
    function warp(uint256) external;
    function prank(address) external;
}

contract Token {
    string public name = "Test";
    uint8 public decimals = 6;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    /// Basis points withheld on every transfer, to model a fee-on-transfer
    /// token. Zero for an ordinary one.
    uint256 public feeBps;

    constructor(uint256 fee) {
        feeBps = fee;
    }

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        return _move(msg.sender, to, amount);
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        require(allowance[from][msg.sender] >= amount, "allowance");
        allowance[from][msg.sender] -= amount;
        return _move(from, to, amount);
    }

    function _move(address from, address to, uint256 amount) private returns (bool) {
        require(balanceOf[from] >= amount, "balance");
        uint256 fee = (amount * feeBps) / 10_000;
        balanceOf[from] -= amount;
        balanceOf[to] += amount - fee;
        return true;
    }
}

contract DedaloClaimTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    DedaloClaim claimer;
    Token token;

    // --- Vectors produced by `dedalo::merkle`, pinned here -------------------
    //
    // This is the point of the file. Two implementations of one encoding is
    // two chances to get it wrong; agreeing on a root and five proofs computed
    // by the other one is what says they do not.
    //
    // Five claims, so the tree has an odd level and exercises the promoted
    // node — the case a duplicate-the-last-node implementation gets wrong.

    bytes16 constant PLAN = bytes16(0x00112233445566778899aabbccddeeff);
    bytes32 constant ROOT = 0xffcf57755a292ee72605206f2e2fe131b222cb0ebd45c0844f68a187a384ec72;
    uint256 constant TOTAL = 4007;

    function _accounts() private pure returns (address[5] memory a) {
        a[0] = 0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed;
        a[1] = 0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359;
        a[2] = 0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB;
        a[3] = 0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb;
        a[4] = 0x9d33df7B2951b0086D40814475869BE3A485a146;
    }

    function _amounts() private pure returns (uint256[5] memory a) {
        a[0] = 1000;
        a[1] = 2500;
        a[2] = 400;
        a[3] = 100;
        a[4] = 7;
    }

    function _proof(uint256 index) private pure returns (bytes32[] memory proof) {
        if (index == 4) {
            proof = new bytes32[](1);
            proof[0] = 0x3c10e56238056db4830e9497627d8c89435b42a0bc7142797badd5ec307a4af2;
            return proof;
        }
        proof = new bytes32[](3);
        if (index == 0) {
            proof[0] = 0xfc5edd4a1fc1f4324bcbb5baa5314f08da758eb87d1f5e805bdb62888ced4103;
            proof[1] = 0xbb63fdb7acb073e28cfa14c1eddbc700ffdd8c83f028b7fc4c317be8f391e6a2;
        } else if (index == 1) {
            proof[0] = 0xc285ac2f86e35db53e495dd5c9f5c14377d686735f1dd08928c36e2a51467166;
            proof[1] = 0xbb63fdb7acb073e28cfa14c1eddbc700ffdd8c83f028b7fc4c317be8f391e6a2;
        } else if (index == 2) {
            proof[0] = 0xcfddde4f42b48651081968e095d417d12fc403218c0ef9175c71dafc7b89c845;
            proof[1] = 0x8744e01a4ec405c244f037e4056b784d96f955ade7b0b24b3cef6340db43268b;
        } else {
            proof[0] = 0x882827f712a338f3617721acd74cb7d8183be9edfe067bdc5cb8db60262e7954;
            proof[1] = 0x8744e01a4ec405c244f037e4056b784d96f955ade7b0b24b3cef6340db43268b;
        }
        proof[2] = 0x5e3abc22b830c6906076486238b2f41f3eb3b577a0d8b3d4c00f4358d9395679;
    }

    function setUp() public {
        claimer = new DedaloClaim();
        token = new Token(0);
        token.mint(address(this), 1_000_000);
        token.approve(address(claimer), type(uint256).max);
    }

    function _deposit() private {
        claimer.deposit(PLAN, ROOT, address(token), TOTAL);
    }

    function _expectRevert(bytes memory call) private {
        (bool ok,) = address(claimer).call(call);
        require(!ok, "expected a revert");
    }

    /// The cross-check: every proof `dedalo::merkle` produced is accepted, and
    /// every contributor ends up with exactly what the plan said.
    function test_RustProofsVerifyAndPayExactly() public {
        _deposit();
        address[5] memory accounts = _accounts();
        uint256[5] memory amounts = _amounts();

        for (uint256 i = 0; i < 5; i++) {
            claimer.claim(PLAN, i, accounts[i], amounts[i], _proof(i));
            require(token.balanceOf(accounts[i]) == amounts[i], "wrong amount paid");
            require(claimer.claimed(PLAN, i), "index not marked claimed");
        }
        require(token.balanceOf(address(claimer)) == 0, "the round must be emptied exactly");
    }

    /// The replay guard the whole idempotency story rests on.
    function test_DepositingTheSamePlanTwiceReverts() public {
        _deposit();
        _expectRevert(
            abi.encodeCall(DedaloClaim.deposit, (PLAN, ROOT, address(token), TOTAL))
        );
    }

    function test_ClaimingTwiceReverts() public {
        _deposit();
        address[5] memory accounts = _accounts();
        uint256[5] memory amounts = _amounts();
        claimer.claim(PLAN, 0, accounts[0], amounts[0], _proof(0));
        _expectRevert(
            abi.encodeCall(DedaloClaim.claim, (PLAN, 0, accounts[0], amounts[0], _proof(0)))
        );
    }

    function test_InflatingTheAmountReverts() public {
        _deposit();
        address[5] memory accounts = _accounts();
        _expectRevert(
            abi.encodeCall(DedaloClaim.claim, (PLAN, 0, accounts[0], 999_999, _proof(0)))
        );
    }

    function test_RedirectingToAnotherAccountReverts() public {
        _deposit();
        uint256[5] memory amounts = _amounts();
        _expectRevert(
            abi.encodeCall(DedaloClaim.claim, (PLAN, 0, address(0xdead), amounts[0], _proof(0)))
        );
    }

    function test_UsingAnotherClaimsProofReverts() public {
        _deposit();
        address[5] memory accounts = _accounts();
        uint256[5] memory amounts = _amounts();
        _expectRevert(
            abi.encodeCall(DedaloClaim.claim, (PLAN, 0, accounts[0], amounts[0], _proof(1)))
        );
    }

    function test_ClaimingAnUnknownRoundReverts() public {
        address[5] memory accounts = _accounts();
        uint256[5] memory amounts = _amounts();
        _expectRevert(
            abi.encodeCall(DedaloClaim.claim, (PLAN, 0, accounts[0], amounts[0], _proof(0)))
        );
    }

    function test_SweepBeforeExpiryReverts() public {
        _deposit();
        _expectRevert(abi.encodeCall(DedaloClaim.sweep, (PLAN)));
    }

    function test_SweepByAnyoneElseReverts() public {
        _deposit();
        vm.warp(block.timestamp + 181 days);
        vm.prank(address(0xbeef));
        _expectRevert(abi.encodeCall(DedaloClaim.sweep, (PLAN)));
    }

    /// Unclaimed money is recoverable, which is the difference between "not
    /// yet claimed" and "destroyed".
    function test_SweepAfterExpiryReturnsOnlyWhatIsLeft() public {
        _deposit();
        address[5] memory accounts = _accounts();
        uint256[5] memory amounts = _amounts();
        claimer.claim(PLAN, 1, accounts[1], amounts[1], _proof(1));

        uint256 before = token.balanceOf(address(this));
        vm.warp(block.timestamp + 181 days);
        claimer.sweep(PLAN);

        require(token.balanceOf(address(this)) - before == TOTAL - amounts[1], "swept the wrong amount");
        require(token.balanceOf(address(claimer)) == 0, "nothing may be left behind");
    }

    /// A token that withholds a fee delivers less than the round promises, so
    /// early claimants would be paid and the rest stranded.
    function test_ShortDeliveryIsRefusedAtDeposit() public {
        Token lossy = new Token(100); // 1%
        lossy.mint(address(this), 1_000_000);
        lossy.approve(address(claimer), type(uint256).max);
        _expectRevert(
            abi.encodeCall(DedaloClaim.deposit, (PLAN, ROOT, address(lossy), TOTAL))
        );
    }

    function test_ZeroRootOrZeroTotalIsRefused() public {
        _expectRevert(
            abi.encodeCall(DedaloClaim.deposit, (PLAN, bytes32(0), address(token), TOTAL))
        );
        _expectRevert(abi.encodeCall(DedaloClaim.deposit, (PLAN, ROOT, address(token), 0)));
    }
}
