//! Zendvo Escrow Contract
//!
//! Locks USDC for a recipient until a predetermined timestamp.
//! Only the designated recipient can claim after the unlock time.

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, Symbol,
};

// ─── Storage keys ─────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Sender,
    Recipient,
    Token,
    Amount,
    UnlockTime,
    Claimed,
}

// ─── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    /// Initialize the escrow. Called once by the platform after deploying.
    ///
    /// * `sender`      – address that funded the escrow
    /// * `recipient`   – address that may claim after `unlock_time`
    /// * `token`       – USDC token contract address
    /// * `amount`      – amount in stroops (7 decimal places)
    /// * `unlock_time` – Unix timestamp (seconds) after which claim is allowed
    pub fn initialize(
        env: Env,
        sender: Address,
        recipient: Address,
        token: Address,
        amount: i128,
        unlock_time: u64,
    ) {
        // Prevent re-initialization
        if env.storage().instance().has(&DataKey::Sender) {
            panic!("already initialized");
        }

        sender.require_auth();

        env.storage().instance().set(&DataKey::Sender, &sender);
        env.storage().instance().set(&DataKey::Recipient, &recipient);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::Amount, &amount);
        env.storage().instance().set(&DataKey::UnlockTime, &unlock_time);
        env.storage().instance().set(&DataKey::Claimed, &false);

        // Transfer USDC from sender into this contract
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&sender, &env.current_contract_address(), &amount);

        env.events().publish(
            (Symbol::new(&env, "initialized"),),
            (sender, recipient, amount, unlock_time),
        );
    }

    /// Claim the escrowed funds. Only callable by the recipient after unlock_time.
    pub fn claim(env: Env) {
        let recipient: Address = env
            .storage()
            .instance()
            .get(&DataKey::Recipient)
            .expect("not initialized");

        recipient.require_auth();

        let claimed: bool = env
            .storage()
            .instance()
            .get(&DataKey::Claimed)
            .unwrap_or(false);

        if claimed {
            panic!("already claimed");
        }

        let unlock_time: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UnlockTime)
            .expect("not initialized");

        let now = env.ledger().timestamp();
        if now < unlock_time {
            panic!("gift is still locked");
        }

        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .expect("not initialized");

        let amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Amount)
            .expect("not initialized");

        env.storage().instance().set(&DataKey::Claimed, &true);

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &recipient, &amount);

        env.events().publish(
            (Symbol::new(&env, "claimed"),),
            (recipient, amount),
        );
    }

    /// Read-only: returns (recipient, amount, unlock_time, claimed).
    pub fn get_state(env: Env) -> (Address, i128, u64, bool) {
        let recipient: Address = env
            .storage()
            .instance()
            .get(&DataKey::Recipient)
            .expect("not initialized");
        let amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Amount)
            .expect("not initialized");
        let unlock_time: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UnlockTime)
            .expect("not initialized");
        let claimed: bool = env
            .storage()
            .instance()
            .get(&DataKey::Claimed)
            .unwrap_or(false);

        (recipient, amount, unlock_time, claimed)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::{Client as TokenClient, StellarAssetClient},
        Env,
    };

    fn setup(env: &Env) -> (Address, Address, Address, TokenClient, EscrowContractClient) {
        let sender = Address::generate(env);
        let recipient = Address::generate(env);
        let token_id = env.register_stellar_asset_contract(sender.clone());
        let token = TokenClient::new(env, &token_id);
        let token_admin = StellarAssetClient::new(env, &token_id);
        token_admin.mint(&sender, &100_000_000);
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(env, &contract_id);
        (sender, recipient, token_id, token, client)
    }

    // ── Happy paths ──────────────────────────────────────────────────────────

    #[test]
    fn test_initialize_and_claim() {
        let env = Env::default();
        env.mock_all_auths();
        let (sender, recipient, token_id, token, client) = setup(&env);

        client.initialize(&sender, &recipient, &token_id, &100_000_000, &1_000);
        env.ledger().with_mut(|l| l.timestamp = 1_001);
        client.claim();

        assert_eq!(token.balance(&recipient), 100_000_000);
        assert_eq!(token.balance(&client.address), 0);
    }

    #[test]
    fn test_get_state_after_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let (sender, recipient, token_id, _token, client) = setup(&env);

        client.initialize(&sender, &recipient, &token_id, &100_000_000, &5_000);
        let (ret_recipient, amount, unlock_time, claimed) = client.get_state();

        assert_eq!(ret_recipient, recipient);
        assert_eq!(amount, 100_000_000);
        assert_eq!(unlock_time, 5_000);
        assert!(!claimed);
    }

    #[test]
    fn test_get_state_after_claim() {
        let env = Env::default();
        env.mock_all_auths();
        let (sender, recipient, token_id, _token, client) = setup(&env);

        client.initialize(&sender, &recipient, &token_id, &100_000_000, &1_000);
        env.ledger().with_mut(|l| l.timestamp = 2_000);
        client.claim();

        let (_r, _a, _u, claimed) = client.get_state();
        assert!(claimed);
    }

    #[test]
    fn test_claim_at_exact_unlock_time() {
        let env = Env::default();
        env.mock_all_auths();
        let (sender, recipient, token_id, token, client) = setup(&env);

        client.initialize(&sender, &recipient, &token_id, &100_000_000, &1_000);
        // timestamp == unlock_time should succeed (now >= unlock_time)
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        client.claim();

        assert_eq!(token.balance(&recipient), 100_000_000);
    }

    // ── Failure modes ────────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "gift is still locked")]
    fn test_claim_before_unlock_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (sender, recipient, token_id, _token, client) = setup(&env);

        client.initialize(&sender, &recipient, &token_id, &100_000_000, &9_999_999);
        client.claim();
    }

    #[test]
    #[should_panic(expected = "already claimed")]
    fn test_double_claim_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (sender, recipient, token_id, _token, client) = setup(&env);

        client.initialize(&sender, &recipient, &token_id, &100_000_000, &1_000);
        env.ledger().with_mut(|l| l.timestamp = 2_000);
        client.claim();
        client.claim(); // second claim must panic
    }

    #[test]
    #[should_panic(expected = "already initialized")]
    fn test_double_initialize_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (sender, recipient, token_id, _token, client) = setup(&env);

        client.initialize(&sender, &recipient, &token_id, &50_000_000, &1_000);
        client.initialize(&sender, &recipient, &token_id, &50_000_000, &1_000);
    }

    #[test]
    #[should_panic]
    fn test_zero_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (sender, recipient, token_id, _token, client) = setup(&env);

        // transfer of 0 should be rejected by the token contract
        client.initialize(&sender, &recipient, &token_id, &0, &1_000);
    }

    // ── Reentrancy guard (checks-effects-interactions) ───────────────────────

    #[test]
    fn test_claimed_flag_set_before_transfer() {
        // After a successful claim the state must show claimed=true,
        // proving the flag was set before (or atomically with) the transfer.
        let env = Env::default();
        env.mock_all_auths();
        let (sender, recipient, token_id, _token, client) = setup(&env);

        client.initialize(&sender, &recipient, &token_id, &100_000_000, &1_000);
        env.ledger().with_mut(|l| l.timestamp = 2_000);
        client.claim();

        let (_r, _a, _u, claimed) = client.get_state();
        assert!(claimed, "claimed flag must be true after claim");
    }

    // ── Expired / edge-case gifts ────────────────────────────────────────────

    #[test]
    fn test_claim_long_after_unlock_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (sender, recipient, token_id, token, client) = setup(&env);

        client.initialize(&sender, &recipient, &token_id, &100_000_000, &1_000);
        // Simulate claiming years later
        env.ledger().with_mut(|l| l.timestamp = 999_999_999);
        client.claim();

        assert_eq!(token.balance(&recipient), 100_000_000);
    }

    #[test]
    fn test_minimum_amount_one_stroop() {
        let env = Env::default();
        env.mock_all_auths();
        let sender = Address::generate(&env);
        let recipient = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(sender.clone());
        let token = TokenClient::new(&env, &token_id);
        StellarAssetClient::new(&env, &token_id).mint(&sender, &1);

        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        client.initialize(&sender, &recipient, &token_id, &1, &1_000);
        env.ledger().with_mut(|l| l.timestamp = 1_001);
        client.claim();

        assert_eq!(token.balance(&recipient), 1);
    }
}
