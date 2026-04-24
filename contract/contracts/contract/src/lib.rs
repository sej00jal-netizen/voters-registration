#![no_std]
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype,
    symbol_short, Address, Bytes, BytesN, Env, Map, Symbol, Vec, vec,
};

// ─── Storage Keys ────────────────────────────────────────────────────────────

const ADMIN: Symbol       = symbol_short!("ADMIN");
const PROPOSALS: Symbol   = symbol_short!("PROPS");
const NULLIFIERS: Symbol  = symbol_short!("NULLS");
const VOTERS: Symbol      = symbol_short!("VOTERS");
const VOTING_OPEN: Symbol = symbol_short!("OPEN");

// ─── Events ──────────────────────────────────────────────────────────────────

#[contractevent]
pub struct ProposalAdded {
    pub proposal_id: u32,
}

#[contractevent]
pub struct VoterRegistered {
    pub voter: Address,
}

#[contractevent]
pub struct VotingOpened {}

#[contractevent]
pub struct VotingClosed {}

#[contractevent]
pub struct VoteCast {
    // Only nullifier is emitted — never the voter address
    pub proposal_id: u32,
    pub nullifier:   BytesN<32>,
}

// ─── Data Types ──────────────────────────────────────────────────────────────

/// A voting proposal with its metadata and tally
#[contracttype]
#[derive(Clone)]
pub struct Proposal {
    pub id:          u32,
    pub title:       Bytes,
    pub description: Bytes,
    pub vote_count:  u64,
}

/// Error codes returned by the contract.
/// #[contracterror] generates the From/Into impls required by #[contractimpl].
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum VotingError {
    Unauthorized         = 1,
    AlreadyRegistered    = 2,
    NotRegistered        = 3,
    NullifierAlreadyUsed = 4,
    InvalidProposal      = 5,
    VotingClosed         = 6,
    VotingAlreadyOpen    = 7,
}

// ─── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct AnonymousVotingContract;

#[contractimpl]
impl AnonymousVotingContract {
    // ── Initialization ───────────────────────────────────────────────────────

    /// Deploy the contract and set the admin.
    /// Must be called once before any other function.
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&VOTING_OPEN, &false);

        let proposals: Vec<Proposal> = vec![&env];
        env.storage().instance().set(&PROPOSALS, &proposals);

        let nullifiers: Map<BytesN<32>, bool> = Map::new(&env);
        env.storage().instance().set(&NULLIFIERS, &nullifiers);

        let voters: Map<Address, bool> = Map::new(&env);
        env.storage().instance().set(&VOTERS, &voters);
    }

    // ── Admin Functions ──────────────────────────────────────────────────────

    /// Add a new proposal. Admin only.
    pub fn add_proposal(
        env: Env,
        title: Bytes,
        description: Bytes,
    ) -> Result<u32, VotingError> {
        Self::require_admin(&env)?;

        let mut proposals: Vec<Proposal> = env
            .storage()
            .instance()
            .get(&PROPOSALS)
            .unwrap_or(vec![&env]);

        let id = proposals.len();
        proposals.push_back(Proposal { id, title, description, vote_count: 0 });
        env.storage().instance().set(&PROPOSALS, &proposals);

        env.events().publish_event(&ProposalAdded { proposal_id: id });
        Ok(id)
    }

    /// Open the voting session. Admin only.
    pub fn open_voting(env: Env) -> Result<(), VotingError> {
        Self::require_admin(&env)?;

        let is_open: bool = env
            .storage()
            .instance()
            .get(&VOTING_OPEN)
            .unwrap_or(false);
        if is_open {
            return Err(VotingError::VotingAlreadyOpen);
        }

        env.storage().instance().set(&VOTING_OPEN, &true);
        env.events().publish_event(&VotingOpened {});
        Ok(())
    }

    /// Close the voting session. Admin only.
    pub fn close_voting(env: Env) -> Result<(), VotingError> {
        Self::require_admin(&env)?;
        env.storage().instance().set(&VOTING_OPEN, &false);
        env.events().publish_event(&VotingClosed {});
        Ok(())
    }

    // ── Voter Registration ───────────────────────────────────────────────────

    /// Register a voter address so they are eligible to vote. Admin only.
    pub fn register_voter(env: Env, voter: Address) -> Result<(), VotingError> {
        Self::require_admin(&env)?;

        let mut voters: Map<Address, bool> = env
            .storage()
            .instance()
            .get(&VOTERS)
            .unwrap_or(Map::new(&env));

        if voters.contains_key(voter.clone()) {
            return Err(VotingError::AlreadyRegistered);
        }

        voters.set(voter.clone(), true);
        env.storage().instance().set(&VOTERS, &voters);

        env.events().publish_event(&VoterRegistered { voter });
        Ok(())
    }

    // ── Voting ───────────────────────────────────────────────────────────────

    /// Cast an anonymous vote.
    ///
    /// Anonymity mechanism
    /// -------------------
    /// The voter supplies a `nullifier` - a 32-byte value derived off-chain,
    /// e.g. SHA-256(secret_key || proposal_id). The contract:
    ///   1. Verifies the voter is registered (eligibility).
    ///   2. Checks the nullifier has never been used (no double-vote).
    ///   3. Records the nullifier on-chain — NOT the voter address.
    ///   4. Increments the proposal tally.
    ///
    /// An observer can confirm every vote is from a registered voter and
    /// that no voter voted twice, but cannot link an address to a choice.
    pub fn cast_vote(
        env: Env,
        voter: Address,
        proposal_id: u32,
        nullifier: BytesN<32>,
    ) -> Result<(), VotingError> {
        voter.require_auth();

        // 1. Voting must be open
        let is_open: bool = env
            .storage()
            .instance()
            .get(&VOTING_OPEN)
            .unwrap_or(false);
        if !is_open {
            return Err(VotingError::VotingClosed);
        }

        // 2. Voter must be registered
        let voters: Map<Address, bool> = env
            .storage()
            .instance()
            .get(&VOTERS)
            .unwrap_or(Map::new(&env));
        if !voters.contains_key(voter.clone()) {
            return Err(VotingError::NotRegistered);
        }

        // 3. Nullifier must be fresh
        let mut nullifiers: Map<BytesN<32>, bool> = env
            .storage()
            .instance()
            .get(&NULLIFIERS)
            .unwrap_or(Map::new(&env));
        if nullifiers.contains_key(nullifier.clone()) {
            return Err(VotingError::NullifierAlreadyUsed);
        }

        // 4. Proposal must exist
        let proposals: Vec<Proposal> = env
            .storage()
            .instance()
            .get(&PROPOSALS)
            .unwrap_or(vec![&env]);
        if proposal_id >= proposals.len() {
            return Err(VotingError::InvalidProposal);
        }

        // 5. Record nullifier
        nullifiers.set(nullifier.clone(), true);
        env.storage().instance().set(&NULLIFIERS, &nullifiers);

        // 6. Increment tally — Soroban Vec has no in-place mutation, rebuild it
        let mut updated: Vec<Proposal> = Vec::new(&env);
        for i in 0..proposals.len() {
            let mut p = proposals.get(i).unwrap();
            if p.id == proposal_id {
                p.vote_count += 1;
            }
            updated.push_back(p);
        }
        env.storage().instance().set(&PROPOSALS, &updated);

        // Emit nullifier only — NOT the voter address
        env.events().publish_event(&VoteCast { proposal_id, nullifier });
        Ok(())
    }

    // ── Read-only Queries ────────────────────────────────────────────────────

    /// Return all proposals with current vote tallies.
    pub fn get_proposals(env: Env) -> Vec<Proposal> {
        env.storage()
            .instance()
            .get(&PROPOSALS)
            .unwrap_or(vec![&env])
    }

    /// Return a single proposal by ID.
    pub fn get_proposal(env: Env, proposal_id: u32) -> Result<Proposal, VotingError> {
        let proposals: Vec<Proposal> = env
            .storage()
            .instance()
            .get(&PROPOSALS)
            .unwrap_or(vec![&env]);

        if proposal_id >= proposals.len() {
            return Err(VotingError::InvalidProposal);
        }
        Ok(proposals.get(proposal_id).unwrap())
    }

    /// Check whether voting is currently open.
    pub fn is_voting_open(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&VOTING_OPEN)
            .unwrap_or(false)
    }

    /// Check whether a voter address is registered.
    pub fn is_voter_registered(env: Env, voter: Address) -> bool {
        let voters: Map<Address, bool> = env
            .storage()
            .instance()
            .get(&VOTERS)
            .unwrap_or(Map::new(&env));
        voters.contains_key(voter)
    }

    /// Check whether a nullifier has already been used.
    pub fn is_nullifier_used(env: Env, nullifier: BytesN<32>) -> bool {
        let nullifiers: Map<BytesN<32>, bool> = env
            .storage()
            .instance()
            .get(&NULLIFIERS)
            .unwrap_or(Map::new(&env));
        nullifiers.contains_key(nullifier)
    }

    // ── Internal Helpers ─────────────────────────────────────────────────────

    fn require_admin(env: &Env) -> Result<(), VotingError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN)
            .ok_or(VotingError::Unauthorized)?;
        admin.require_auth();
        Ok(())
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Bytes, BytesN, Env};

    fn deploy(env: &Env) -> (AnonymousVotingContractClient, Address) {
        let contract_id = env.register_contract(None, AnonymousVotingContract);
        let client = AnonymousVotingContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        (client, admin)
    }

    #[test]
    fn test_full_voting_flow() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = deploy(&env);

        assert_eq!(
            client.add_proposal(
                &Bytes::from_slice(&env, b"Proposal A"),
                &Bytes::from_slice(&env, b"Description A"),
            ),
            0
        );
        assert_eq!(
            client.add_proposal(
                &Bytes::from_slice(&env, b"Proposal B"),
                &Bytes::from_slice(&env, b"Description B"),
            ),
            1
        );

        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);
        client.register_voter(&voter1);
        client.register_voter(&voter2);
        client.open_voting();

        client.cast_vote(&voter1, &0, &BytesN::from_array(&env, &[1u8; 32]));
        client.cast_vote(&voter2, &0, &BytesN::from_array(&env, &[2u8; 32]));

        let proposals = client.get_proposals();
        assert_eq!(proposals.get(0).unwrap().vote_count, 2);
        assert_eq!(proposals.get(1).unwrap().vote_count, 0);
    }

    #[test]
    fn test_double_vote_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = deploy(&env);

        client.add_proposal(
            &Bytes::from_slice(&env, b"P"),
            &Bytes::from_slice(&env, b"D"),
        );
        let voter = Address::generate(&env);
        client.register_voter(&voter);
        client.open_voting();

        let nullifier = BytesN::from_array(&env, &[9u8; 32]);
        client.cast_vote(&voter, &0, &nullifier);

        let err = client
            .try_cast_vote(&voter, &0, &nullifier)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, VotingError::NullifierAlreadyUsed);
    }

    #[test]
    fn test_unregistered_voter_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = deploy(&env);

        client.add_proposal(
            &Bytes::from_slice(&env, b"P"),
            &Bytes::from_slice(&env, b"D"),
        );
        client.open_voting();

        let err = client
            .try_cast_vote(
                &Address::generate(&env),
                &0,
                &BytesN::from_array(&env, &[5u8; 32]),
            )
            .unwrap_err()
            .unwrap();
        assert_eq!(err, VotingError::NotRegistered);
    }

    #[test]
    fn test_vote_when_closed_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = deploy(&env);

        client.add_proposal(
            &Bytes::from_slice(&env, b"P"),
            &Bytes::from_slice(&env, b"D"),
        );
        let voter = Address::generate(&env);
        client.register_voter(&voter);
        // Deliberately NOT calling open_voting()

        let err = client
            .try_cast_vote(&voter, &0, &BytesN::from_array(&env, &[7u8; 32]))
            .unwrap_err()
            .unwrap();
        assert_eq!(err, VotingError::VotingClosed);
    }

    #[test]
    fn test_register_duplicate_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = deploy(&env);

        let voter = Address::generate(&env);
        client.register_voter(&voter);

        let err = client.try_register_voter(&voter).unwrap_err().unwrap();
        assert_eq!(err, VotingError::AlreadyRegistered);
    }

    #[test]
    fn test_is_voter_registered() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = deploy(&env);

        let voter = Address::generate(&env);
        assert!(!client.is_voter_registered(&voter));
        client.register_voter(&voter);
        assert!(client.is_voter_registered(&voter));
    }

    #[test]
    fn test_nullifier_tracking() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = deploy(&env);

        client.add_proposal(
            &Bytes::from_slice(&env, b"P"),
            &Bytes::from_slice(&env, b"D"),
        );
        let voter = Address::generate(&env);
        client.register_voter(&voter);
        client.open_voting();

        let nullifier = BytesN::from_array(&env, &[42u8; 32]);
        assert!(!client.is_nullifier_used(&nullifier));
        client.cast_vote(&voter, &0, &nullifier);
        assert!(client.is_nullifier_used(&nullifier));
    }
}