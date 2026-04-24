# 🗳️ Anonymous Voting — Soroban Smart Contract

> A privacy-preserving on-chain voting system built on [Stellar](https://stellar.org) using [Soroban](https://soroban.stellar.org).  
> Voters are verified for eligibility, yet their individual choices remain completely anonymous.

---

## 📋 Project Description

**Anonymous Voting** is a Soroban smart contract that brings trustless, tamper-proof elections to the Stellar blockchain. It solves a fundamental tension in on-chain governance: _how do you prove someone is eligible to vote, while still hiding who voted for what?_

The answer used here is a **nullifier-based anonymity scheme**. Every registered voter generates a secret off-chain value and derives a unique one-way commitment (nullifier) from it before submitting their vote. The contract records only the nullifier — never the voter's address alongside their choice — so no one can reconstruct the mapping between identity and ballot.

The contract is fully on-chain with no oracle or trusted off-chain component beyond the voter's own secret management.

---

## ⚙️ What It Does

### Lifecycle overview

```
Admin deploys → initialize()
     │
     ├─► add_proposal()   (one or more times)
     ├─► register_voter() (for each eligible voter)
     └─► open_voting()
              │
              ├─► cast_vote()  ← voters call this
              │
         close_voting()
              │
         get_proposals()  ← anyone reads final tallies
```

### Step-by-step

| Step | Who | Function | What happens |
|------|-----|----------|--------------|
| 1 | Admin | `initialize(admin)` | Deploys state, sets admin |
| 2 | Admin | `add_proposal(title, description)` | Creates a ballot option; returns its numeric ID |
| 3 | Admin | `register_voter(voter_address)` | Whitelists an eligible voter |
| 4 | Admin | `open_voting()` | Opens the voting window |
| 5 | Voter | `cast_vote(voter, proposal_id, nullifier)` | Casts an anonymous vote (see below) |
| 6 | Admin | `close_voting()` | Ends the voting window |
| 7 | Anyone | `get_proposals()` | Reads final tallies |

### How anonymity works

When a voter wants to cast a ballot they:

1. **Off-chain**: generate a random secret key and compute  
   `nullifier = SHA-256(secret_key ∥ proposal_id)`
2. **On-chain**: call `cast_vote(their_address, proposal_id, nullifier)`

The contract:
- Verifies the address is registered (proves eligibility via `require_auth`)
- Checks the nullifier has never been seen (prevents double-voting)
- Stores **only the nullifier** — not `(address, proposal_id)` together
- Increments the proposal's tally by 1
- Emits an event containing `(proposal_id, nullifier)` — not the voter address

Because the on-chain record is `nullifier → used`, an observer can verify that every vote came from a registered voter and that no voter voted twice, but **cannot determine which registered voter chose which proposal**.

---

## ✨ Features

### 🔐 Privacy-preserving votes
Votes are recorded via nullifier hashes. The contract never stores a link between a voter's address and their chosen proposal. Blockchain observers and even the contract admin cannot reconstruct individual ballots.

### 🚫 Double-vote prevention
Each nullifier can only be used once. Any attempt to reuse a nullifier — even from a different address — is rejected with `NullifierAlreadyUsed`. The nullifier set is persisted in contract storage across all invocations.

### ✅ Eligibility enforcement
Only addresses whitelisted by the admin via `register_voter` can cast votes. Unregistered addresses are rejected with `NotRegistered`, ensuring only authorised participants can influence the outcome.

### 📅 Controlled voting windows
The admin explicitly opens and closes the voting session. Votes submitted before `open_voting()` or after `close_voting()` are rejected with `VotingClosed`, giving the organiser full control over timing.

### 📜 Multiple proposals
Any number of proposals can be added before voting opens. Each proposal maintains an independent tally, supporting multi-option referendums and ranked-choice extensions.

### 🔔 On-chain event log
Every significant action emits a Soroban event (`prop_add`, `reg`, `vote`, `v_open`, `v_close`). Vote events carry only the proposal ID and nullifier — no voter address — so the audit trail is transparent without sacrificing privacy.

### 🛡️ Role-based access control
Sensitive functions (`add_proposal`, `register_voter`, `open_voting`, `close_voting`) require admin authorisation enforced by Soroban's native `require_auth`. The admin is set once at deploy time and cannot be changed.

### 🧪 Comprehensive test suite
The contract ships with unit tests covering the happy path, double-vote rejection, unregistered voter rejection, and voting-while-closed rejection — all runnable with `cargo test`.

---

## 🗂️ Project Structure

```
anonymous_voting/
├── Cargo.toml                      # Workspace manifest
└── contracts/
    └── voting/
        ├── Cargo.toml              # Contract crate manifest
        └── src/
            └── lib.rs              # Contract source + tests
```

---

## 🚀 Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable, 1.74+)
- [Soroban CLI](https://soroban.stellar.org/docs/getting-started/setup)

```bash
cargo install --locked soroban-cli
rustup target add wasm32-unknown-unknown
```

### Build

```bash
cd anonymous_voting
cargo build --release --target wasm32-unknown-unknown
```

The compiled WASM artefact will be at:
```
target/wasm32-unknown-unknown/release/anonymous_voting.wasm
```

### Run tests

```bash
cargo test
```

### Deploy to Testnet

```bash
# Configure Testnet identity
soroban keys generate --global alice --network testnet

# Deploy
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/anonymous_voting.wasm \
  --source alice \
  --network testnet
```

### Invoke — example

```bash
# Initialize
soroban contract invoke --id <CONTRACT_ID> --source alice --network testnet \
  -- initialize --admin <ADMIN_ADDRESS>

# Add a proposal
soroban contract invoke --id <CONTRACT_ID> --source alice --network testnet \
  -- add_proposal \
     --title "$(echo -n 'Increase validator rewards' | xxd -p)" \
     --description "$(echo -n 'Raise rewards from 5% to 7%' | xxd -p)"

# Register a voter
soroban contract invoke --id <CONTRACT_ID> --source alice --network testnet \
  -- register_voter --voter <VOTER_ADDRESS>

# Open voting
soroban contract invoke --id <CONTRACT_ID> --source alice --network testnet \
  -- open_voting

# Cast a vote (nullifier generated off-chain)
soroban contract invoke --id <CONTRACT_ID> --source voter --network testnet \
  -- cast_vote \
     --voter <VOTER_ADDRESS> \
     --proposal_id 0 \
     --nullifier <32_BYTE_HEX_NULLIFIER>

# Read results
soroban contract invoke --id <CONTRACT_ID> --network testnet \
  -- get_proposals
```

---

## 🔒 Security Notes

- **Secret management**: The security of vote privacy depends on the voter keeping their secret key secret. If a voter reveals their secret, anyone can derive their nullifier and identify their vote.
- **Registration linkage**: The act of registering a voter is public. This contract proves _that_ a registered voter voted, but not _how_ they voted.
- **Admin trust**: The admin controls voter registration and voting windows. A fully trustless system would replace the admin with a DAO or multisig.
- **No ZK proofs**: This scheme uses commitment-based anonymity, not zero-knowledge proofs. It is simpler but assumes voters manage secrets responsibly.

---


wallet address: GAUXYEULKZE55NYQHPDBMXVWQI6XH22GOB5UY2G6CZ3B7SDYBQEQZA3H

contract address: CBGQCYQ2NOY2XCPUSNFE3O4Z3AN76XYON4FLVSCEDBOJ57QNL2UF65TA

https://stellar.expert/explorer/testnet/contract/CBGQCYQ2NOY2XCPUSNFE3O4Z3AN76XYON4FLVSCEDBOJ57QNL2UF65TA

<img width="1881" height="965" alt="image" src="https://github.com/user-attachments/assets/5de7c1a0-92ea-4ccc-9880-58d41b653c51" />
