use std::collections::HashMap;

pub use account::Account;
pub use transaction::{LoggedTransaction, State, Transaction};

mod account;
mod transaction;

/// A ledger represents a store of financial accuonts along with the transactions for each account
#[derive(Debug)]
pub struct Ledger {
    accounts: HashMap<u16, Account>,
    log: HashMap<u32, LoggedTransaction>,
}

pub trait Process {
    type Output;
    fn process(self, ledger: &mut Ledger) -> Self::Output;
}

impl Ledger {
    /// Create a new empty ledger
    pub fn new() -> Self {
        Ledger {
            accounts: HashMap::new(),
            log: HashMap::new(),
        }
    }

    /// Find an account/transaction pair, used to split a mutable reference into
    /// a mutable reference for each field (since the borrow checker is smart about
    /// struct fields
    pub fn find_account_and_transaction(
        &mut self,
        client_id: u16,
        tx_id: u32,
    ) -> (Option<&mut Account>, Option<&mut LoggedTransaction>) {
        (self.accounts.get_mut(&client_id), self.log.get_mut(&tx_id))
    }

    /// Find an account in the ledger, returning a mutable reference if an account is found, otherwise `None`
    pub fn find_account(&mut self, id: u16) -> Option<&mut Account> {
        self.accounts.get_mut(&id)
    }

    /// Find an account or create a new one if it doesn't exist
    pub fn find_or_create_account(&mut self, id: u16) -> &mut Account {
        self.accounts.entry(id).or_insert_with(|| Account::new(id))
    }

    /// Log a transaction in the ledger as had being completed
    pub fn log_transaction(&mut self, id: u32, tx: Transaction) {
        self.log.insert(id, LoggedTransaction::new(tx));
    }

    pub fn accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.values()
    }

    pub fn process<P: Process>(&mut self, p: P) -> P::Output {
        p.process(self)
    }
}
