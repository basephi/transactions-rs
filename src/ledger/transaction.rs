use std::fmt::{Debug, Display, Formatter};

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Deserializer};

use crate::ledger::Account;

use super::{Ledger, Process};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Transaction {
    Deposit(Deposit),
    Withdrawal(Withdrawal),
    Dispute(Dispute),
    Resolve(Resolve),
    Chargeback(Chargeback),
    Unknown {
        client_id: u16,
        tx_id: u32,
        amount: Decimal,
    },
}

/// The return value for when a transaction is processed
type Receipt = ();

impl Process for Transaction {
    type Output = Result<Receipt>;

    fn process(self, ledger: &mut Ledger) -> Result<Receipt> {
        match self {
            Transaction::Deposit(deposit) => deposit.process(ledger),
            Transaction::Withdrawal(withdrawal) => withdrawal.process(ledger),
            Transaction::Dispute(dispute) => dispute.process(ledger),
            Transaction::Resolve(resolve) => resolve.process(ledger),
            Transaction::Chargeback(chargeback) => chargeback.process(ledger),
            Transaction::Unknown { .. } => Err(Error::UnknownTransactionType),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Deposit {
    client_id: u16,
    tx_id: u32,
    amount: Decimal,
}

impl Process for Deposit {
    type Output = Result<Receipt>;

    fn process(self, ledger: &mut Ledger) -> Result<Receipt> {
        if self.amount <= dec!(0) {
            return Err(Error::InvalidAmount);
        }

        let account = ledger.find_or_create_account(self.client_id);
        if account.locked {
            return Err(Error::AccountLocked);
        }

        account.available += self.amount;
        account.total += self.amount;
        ledger.log_transaction(self.tx_id, Transaction::Deposit(self));
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Withdrawal {
    pub client_id: u16,
    pub tx_id: u32,
    pub amount: Decimal,
}

impl Process for Withdrawal {
    type Output = Result<Receipt>;

    fn process(self, ledger: &mut Ledger) -> Result<Receipt> {
        if self.amount <= dec!(0) {
            return Err(Error::InvalidAmount);
        }

        let account = match ledger.find_account(self.client_id) {
            None => return Err(Error::AccountNotFound),
            Some(account) => account,
        };
        if account.locked {
            return Err(Error::AccountLocked);
        }
        if account.available < self.amount {
            return Err(Error::InsufficientFunds {
                available: account.available,
            });
        }

        account.available -= self.amount;
        account.total -= self.amount;
        ledger.log_transaction(self.tx_id, Transaction::Withdrawal(self));
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Dispute {
    client_id: u16,
    tx_id: u32,
}

impl Process for Dispute {
    type Output = Result<Receipt>;

    fn process(self, ledger: &mut Ledger) -> Result<Receipt> {
        let (account, lt) = ledger.get_account_transaction_mut(self.client_id, self.tx_id)?;

        if lt.transaction.client_id() != self.client_id {
            return Err(Error::MismatchedClient);
        }

        if lt.state != State::Processed {
            return Err(Error::InvalidTransactionState { got: lt.state });
        }

        match lt.transaction {
            Transaction::Deposit(deposit) => {
                lt.state = State::Disputed;
                account.available -= deposit.amount;
                account.held += deposit.amount;
                Ok(())
            }
            _ => Err(Error::InvalidTransactionState { got: lt.state }),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Resolve {
    client_id: u16,
    tx_id: u32,
}

impl Process for Resolve {
    type Output = Result<Receipt>;

    fn process(self, ledger: &mut Ledger) -> Result<Receipt> {
        let (account, lt) = ledger.get_account_transaction_mut(self.client_id, self.tx_id)?;

        if lt.transaction.client_id() != self.client_id {
            return Err(Error::MismatchedClient);
        }
        if lt.state != State::Disputed {
            return Err(Error::InvalidTransactionState { got: lt.state });
        }

        match lt.transaction {
            Transaction::Deposit(deposit) => {
                lt.state = State::Processed;
                account.available += deposit.amount;
                account.held -= deposit.amount;
                Ok(())
            }
            _ => Err(Error::InvalidTransactionState { got: lt.state }),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Chargeback {
    client_id: u16,
    tx_id: u32,
}

impl Process for Chargeback {
    type Output = Result<Receipt>;

    fn process(self, ledger: &mut Ledger) -> Result<Receipt> {
        let (account, lt) = ledger.get_account_transaction_mut(self.client_id, self.tx_id)?;

        if lt.transaction.client_id() != self.client_id {
            return Err(Error::MismatchedClient);
        }
        if lt.state != State::Disputed {
            return Err(Error::InvalidTransactionState { got: lt.state });
        }

        match lt.transaction {
            Transaction::Deposit(deposit) => {
                lt.state = State::Chargeback;
                account.held -= deposit.amount;
                account.total -= deposit.amount;
                account.locked = true;
                Ok(())
            }
            _ => Err(Error::InvalidTransactionState { got: lt.state }),
        }
    }
}

// Add some helper functions to our ledger for transaction error handling
impl Ledger {
    /// Get an account and logged transaction pair. Validates that the transaction and
    /// account ids match correctly
    fn get_account_transaction_mut(
        &mut self,
        client_id: u16,
        tx_id: u32,
    ) -> Result<(&mut Account, &mut LoggedTransaction)> {
        match self.find_account_and_transaction(client_id, tx_id) {
            (None, _) => Err(Error::AccountNotFound),
            (_, None) => Err(Error::TransactionNotFound),
            (Some(a), Some(lt)) => Ok((a, lt)),
        }
    }
}

/// Error types for when a transaction could not be processed properly
#[derive(Debug, PartialEq)]
pub enum Error {
    /// When a withdraw is made but the client doesn't have enough available funds
    InsufficientFunds { available: Decimal },
    /// When a dispute, resolution, or chargeback was made but the impacted transaction wasn't found
    TransactionNotFound,
    /// When a dispute, resolution, or chargeback was made on a transaction in the incorrect state
    /// e.g. trying to dispute a withdrawal, or resolve a transaction that wasn't disputed
    InvalidTransactionState { got: State },
    /// The account for the transaction was not found
    AccountNotFound,
    /// The transaction requested an invalid amount e.g. withdrawing negative values
    InvalidAmount,
    /// When a dispute, resolution, or chargeback references a valid transaction, but
    /// the client id doesn't match the client id of the logged transaction
    MismatchedClient,
    /// All transactions fail if the account is locked (see assumptions in README)
    AccountLocked,
    /// The type of the transaaction was unknown, we cannot process this
    UnknownTransactionType,
}

pub type Result<T> = std::result::Result<T, Error>;

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use Error::*;

        match self {
            InsufficientFunds { available } => {
                write!(f, "insufficient funds, available: '{}'", available)
            }
            TransactionNotFound => write!(f, "transaction not found"),
            InvalidTransactionState { got } => {
                write!(f, "transaction is in the incorrect state: '{:?}'", got)
            }
            AccountNotFound => write!(f, "account not found"),
            InvalidAmount => write!(f, "amount must be a positive number"),
            MismatchedClient => {
                write!(f, "the client id does not match the one on the transaction")
            }
            AccountLocked => write!(f, "the account is locked"),
            UnknownTransactionType => write!(f, "the transaction used an unknown transaction type"),
        }
    }
}

/// The possible states a transaction can be in when logged
#[derive(Debug, PartialEq, Eq, Copy, Clone, Deserialize)]
pub enum State {
    /// The transaction has been processed, and is valid
    Processed,
    /// The transaction has been disputed, funds are on hold
    Disputed,
    /// The transaction has been charged back, funds were removed and account is locked
    Chargeback,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LoggedTransaction {
    transaction: Transaction,
    state: State,
}

impl LoggedTransaction {
    pub fn new(tx: Transaction) -> Self {
        Self {
            transaction: tx,
            state: State::Processed,
        }
    }
}

fn default_if_empty<'de, D, T>(de: D) -> std::result::Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de> + Default,
{
    Option::<T>::deserialize(de).map(|x| x.unwrap_or_default())
}

// This is to make printing our transactions a bit nicer due to our wrapped types
impl Display for Transaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Transaction::Deposit(d) => Debug::fmt(d, f),
            Transaction::Withdrawal(w) => Debug::fmt(w, f),
            Transaction::Dispute(d) => Debug::fmt(d, f),
            Transaction::Resolve(r) => Debug::fmt(r, f),
            Transaction::Chargeback(c) => Debug::fmt(c, f),
            Transaction::Unknown { .. } => Debug::fmt(self, f),
        }
    }
}

impl Transaction {
    fn client_id(self) -> u16 {
        match self {
            Transaction::Deposit(d) => d.client_id,
            Transaction::Withdrawal(w) => w.client_id,
            Transaction::Dispute(d) => d.client_id,
            Transaction::Resolve(r) => r.client_id,
            Transaction::Chargeback(c) => c.client_id,
            Transaction::Unknown { client_id, .. } => client_id,
        }
    }
}

// This exists to create type safety in our transactions -- a Dispute only
// cares about a client id and transaction id, so it doesn't make sense for an
// amount to exist which may be read and used incorrectly. Note that this system
// added a bit of bloat to the end product, check out the README for more info
impl<'de> Deserialize<'de> for Transaction {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Debug, Copy, Clone, Deserialize)]
        pub struct TxIntermediate {
            #[serde(rename = "type")]
            pub kind: TxKind,
            #[serde(rename = "client")]
            pub client_id: u16,
            #[serde(rename = "tx")]
            pub transaction_id: u32,
            #[serde(deserialize_with = "default_if_empty")]
            pub amount: Decimal,
        }

        #[derive(Debug, PartialEq, Eq, Copy, Clone, Deserialize)]
        #[serde(rename_all = "lowercase")]
        pub enum TxKind {
            Deposit,
            Withdrawal,
            Dispute,
            Resolve,
            Chargeback,
            #[serde(other)]
            Unknown,
        }

        let i = TxIntermediate::deserialize(deserializer)?;
        match i.kind {
            TxKind::Deposit => Ok(Transaction::Deposit(Deposit {
                client_id: i.client_id,
                tx_id: i.transaction_id,
                amount: i.amount,
            })),
            TxKind::Withdrawal => Ok(Transaction::Withdrawal(Withdrawal {
                client_id: i.client_id,
                tx_id: i.transaction_id,
                amount: i.amount,
            })),
            TxKind::Dispute => Ok(Transaction::Dispute(Dispute {
                client_id: i.client_id,
                tx_id: i.transaction_id,
            })),
            TxKind::Resolve => Ok(Transaction::Resolve(Resolve {
                client_id: i.client_id,
                tx_id: i.transaction_id,
            })),
            TxKind::Chargeback => Ok(Transaction::Chargeback(Chargeback {
                client_id: i.client_id,
                tx_id: i.transaction_id,
            })),
            TxKind::Unknown => Ok(Transaction::Unknown {
                client_id: i.client_id,
                tx_id: i.transaction_id,
                amount: i.amount,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::prelude::*;
    use rust_decimal_macros::dec;

    use super::*;

    fn build_ledger() -> Ledger {
        let mut ledger = Ledger::new();
        let tx_result = ledger.process(deposit(1, 1, 18.into()));
        assert!(tx_result.is_ok());
        ledger
    }

    #[test]
    fn test_deposit_new_client() {
        let mut ledger = build_ledger();
        let deposit = ledger.process(deposit(13, 4, Decimal::new(51234, 4)));
        assert!(deposit.is_ok());

        let account = ledger.find_account(13);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 13,
                available: Decimal::new(51234, 4),
                held: 0.into(),
                total: Decimal::new(51234, 4),
                locked: false,
            }
        );
    }

    #[test]
    fn test_deposit_existing_client() {
        let mut ledger = build_ledger();
        let deposit = ledger.process(deposit(1, 4, Decimal::new(51234, 4)));
        assert!(deposit.is_ok());

        let account = ledger.find_account(1);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 1,
                available: Decimal::new(231234, 4),
                held: 0.into(),
                total: Decimal::new(231234, 4),
                locked: false,
            }
        );
    }

    #[test]
    fn test_deposit_invalid_amount() {
        let mut ledger = build_ledger();

        // Depositing nothing
        let result = ledger.process(deposit(1, 3, dec!(0)));
        assert_eq!(result, Err(Error::InvalidAmount));

        // Depositing negative
        let result = ledger.process(deposit(1, 3, dec!(-1.5)));
        assert_eq!(result, Err(Error::InvalidAmount));

        // make sure the account hasn't changed
        let account = ledger.find_account(1);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 1,
                available: dec!(18),
                held: 0.into(),
                total: dec!(18),
                locked: false,
            }
        );
    }

    #[test]
    fn test_withdraw_success() {
        let mut ledger = build_ledger();
        let withdraw = ledger.process(withdraw(1, 4, dec!(12.5111)));
        assert!(withdraw.is_ok());

        let account = ledger.find_account(1);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 1,
                available: dec!(5.4889),
                held: 0.into(),
                total: dec!(5.4889),
                locked: false,
            }
        );
    }

    #[test]
    fn test_withdraw_account_not_found() {
        let mut ledger = build_ledger();
        let result = ledger.process(withdraw(2, 4, dec!(2)));
        assert_eq!(result, Err(Error::AccountNotFound));
    }

    #[test]
    fn test_withdraw_insufficient_funds() {
        let mut ledger = build_ledger();
        let result = ledger.process(withdraw(1, 4, dec!(18.5111)));
        assert_eq!(
            result,
            Err(Error::InsufficientFunds {
                available: dec!(18)
            })
        );

        // make sure the account hasn't changed
        let account = ledger.find_account(1);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 1,
                available: dec!(18),
                held: 0.into(),
                total: dec!(18),
                locked: false,
            }
        );
    }

    #[test]
    fn test_withdraw_invalid_amount() {
        let mut ledger = build_ledger();

        // Depositing nothing
        let result = ledger.process(withdraw(1, 3, dec!(0)));
        assert_eq!(result, Err(Error::InvalidAmount));

        // Depositing negative
        let result = ledger.process(withdraw(1, 3, dec!(-1.5)));
        assert_eq!(result, Err(Error::InvalidAmount));

        // make sure the account hasn't changed
        let account = ledger.find_account(1);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 1,
                available: dec!(18),
                held: 0.into(),
                total: dec!(18),
                locked: false,
            }
        );
    }

    #[test]
    fn test_dispute_success() {
        let mut ledger = build_ledger();
        let dispute = ledger.process(dispute(1, 1));
        assert!(dispute.is_ok());
        assert_eq!(ledger.log.get(&1).unwrap().state, State::Disputed);

        let account = ledger.find_account(1);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 1,
                available: 0.into(),
                held: 18.into(),
                total: 18.into(),
                locked: false,
            }
        );
    }

    #[test]
    fn test_dispute_invalid_transaction() {
        let mut ledger = build_ledger();
        let withdrawal = ledger.process(withdraw(1, 2, dec!(10)));
        assert!(withdrawal.is_ok());
        let dispute1 = ledger.process(dispute(1, 1));
        assert!(dispute1.is_ok());

        // dispute on a withdrawal
        match ledger.process(dispute(1, 2)) {
            Err(Error::InvalidTransactionState { .. }) => {}
            r => panic!("expected InvalidTransactionState error, got {:?}", r),
        }

        // dispute on an already disputed transaction
        match ledger.process(dispute(1, 1)) {
            Err(Error::InvalidTransactionState { .. }) => {}
            r => panic!("expected InvalidTransactionState error, got {:?}", r),
        }

        // verify the state of the account
        let account = ledger.find_account(1);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 1,
                available: dec!(-10),
                held: dec!(18),
                total: dec!(8),
                locked: false,
            }
        );
    }

    #[test]
    fn test_dispute_account_not_found() {
        let mut ledger = build_ledger();
        let result = ledger.process(dispute(2, 1));
        assert_eq!(result, Err(Error::AccountNotFound));
    }

    #[test]
    fn test_dispute_transaction_not_found() {
        let mut ledger = build_ledger();
        let result = ledger.process(dispute(1, 2));
        assert_eq!(result, Err(Error::TransactionNotFound));
    }

    #[test]
    fn test_dispute_mismatched_client() {
        let mut ledger = build_ledger();
        let deposit2 = ledger.process(deposit(2, 2, 10.into()));
        assert!(deposit2.is_ok());
        let result = ledger.process(dispute(2, 1));
        assert_eq!(result, Err(Error::MismatchedClient));
    }

    #[test]
    fn test_resolve_success() {
        let mut ledger = build_ledger();
        let dispute = ledger.process(dispute(1, 1));
        assert!(dispute.is_ok());
        let resolve = ledger.process(resolve(1, 1));
        assert!(resolve.is_ok());

        // make sure the transaction went back to the processed state
        assert_eq!(ledger.log.get(&1).unwrap().state, State::Processed);

        let account = ledger.find_account(1);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 1,
                available: dec!(18),
                held: 0.into(),
                total: dec!(18),
                locked: false,
            }
        );
    }

    #[test]
    fn test_resolve_invalid_transaction() {
        let mut ledger = build_ledger();
        let withdrawal = ledger.process(withdraw(1, 2, dec!(10)));
        assert!(withdrawal.is_ok());

        // resolve on a withdrawal
        match ledger.process(resolve(1, 2)) {
            Err(Error::InvalidTransactionState { .. }) => {}
            r => panic!("expected InvalidTransactionState error, got {:?}", r),
        }

        // resolve on an undisputed transaction
        match ledger.process(resolve(1, 1)) {
            Err(Error::InvalidTransactionState { .. }) => {}
            r => panic!("expected InvalidTransactionState error, got {:?}", r),
        }

        // verify the state of the account
        let account = ledger.find_account(1);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 1,
                available: dec!(8),
                held: dec!(0),
                total: dec!(8),
                locked: false,
            }
        );
    }

    #[test]
    fn test_resolve_account_not_found() {
        let mut ledger = build_ledger();
        let result = ledger.process(resolve(2, 1));
        assert_eq!(result, Err(Error::AccountNotFound));
    }

    #[test]
    fn test_resolve_transaction_not_found() {
        let mut ledger = build_ledger();
        let result = ledger.process(resolve(1, 2));
        assert_eq!(result, Err(Error::TransactionNotFound));
    }

    #[test]
    fn test_resolve_mismatched_client() {
        let mut ledger = build_ledger();
        let deposit2 = ledger.process(deposit(2, 2, 10.into()));
        assert!(deposit2.is_ok());
        let dispute = ledger.process(dispute(2, 2));
        assert!(dispute.is_ok());
        let result = ledger.process(resolve(1, 2));
        assert_eq!(result, Err(Error::MismatchedClient));
    }

    #[test]
    fn test_chargeback_success() {
        let mut ledger = build_ledger();
        let dispute = ledger.process(dispute(1, 1));
        assert!(dispute.is_ok());
        let chargeback = ledger.process(chargeback(1, 1));
        assert!(chargeback.is_ok());

        // make sure the transaction went back to the chargebacked state
        assert_eq!(ledger.log.get(&1).unwrap().state, State::Chargeback);

        let account = ledger.find_account(1);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 1,
                available: 0.into(),
                held: 0.into(),
                total: 0.into(),
                locked: true,
            }
        );
    }

    #[test]
    fn test_chargeback_invalid_transaction() {
        let mut ledger = build_ledger();
        let withdrawal = ledger.process(withdraw(1, 2, dec!(10)));
        assert!(withdrawal.is_ok());

        // chargeback on a withdrawal
        match ledger.process(chargeback(1, 2)) {
            Err(Error::InvalidTransactionState { .. }) => {}
            r => panic!("expected InvalidTransactionState error, got {:?}", r),
        }

        // chargeback on an undisputed transaction
        match ledger.process(chargeback(1, 1)) {
            Err(Error::InvalidTransactionState { .. }) => {}
            r => panic!("expected InvalidTransactionState error, got {:?}", r),
        }

        // verify the state of the account
        let account = ledger.find_account(1);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 1,
                available: dec!(8),
                held: dec!(0),
                total: dec!(8),
                locked: false,
            }
        );
    }

    #[test]
    fn test_chargeback_account_not_found() {
        let mut ledger = build_ledger();
        let result = ledger.process(chargeback(2, 1));
        assert_eq!(result, Err(Error::AccountNotFound));
    }

    #[test]
    fn test_chargeback_transaction_not_found() {
        let mut ledger = build_ledger();
        let result = ledger.process(chargeback(1, 2));
        assert_eq!(result, Err(Error::TransactionNotFound));
    }

    #[test]
    fn test_chargeback_mismatched_client() {
        let mut ledger = build_ledger();
        let deposit2 = ledger.process(deposit(2, 2, 10.into()));
        assert!(deposit2.is_ok());
        let dispute = ledger.process(dispute(2, 2));
        assert!(dispute.is_ok());
        let result = ledger.process(chargeback(1, 2));
        assert_eq!(result, Err(Error::MismatchedClient));
    }

    #[test]
    fn test_locked_account() {
        let mut ledger = build_ledger();
        let deposit2 = ledger.process(deposit(1, 2, 10.into()));
        assert!(deposit2.is_ok());
        let deposit3 = ledger.process(deposit(1, 3, 100.into()));
        assert!(deposit3.is_ok());
        let dispute3 = ledger.process(dispute(1, 3));
        assert!(dispute3.is_ok());

        let dispute1 = ledger.process(dispute(1, 1));
        assert!(dispute1.is_ok());
        let chargeback1 = ledger.process(chargeback(1, 1));
        assert!(chargeback1.is_ok());

        // Deposits and withdrawals should be blocked
        assert_eq!(
            ledger.process(deposit(1, 5, 13.into())),
            Err(Error::AccountLocked)
        );
        assert_eq!(
            ledger.process(withdraw(1, 2, 1.into())),
            Err(Error::AccountLocked)
        );

        // Future disputes and chargebacks will continue to be monitored for our
        // balance sheet
        assert!(ledger.process(dispute(1, 2)).is_ok());
        assert!(ledger.process(resolve(1, 3)).is_ok());
        assert!(ledger.process(chargeback(1, 2)).is_ok());

        // Deposit 18, 10, 100
        // 18 is disputed + charged back
        // 10 is disputed + charged back
        // 100 is disputed + resolved
        let account = ledger.find_account(1);
        assert!(account.is_some());
        assert_eq!(
            account.unwrap(),
            &Account {
                client: 1,
                available: 100.into(),
                held: 0.into(),
                total: 100.into(),
                locked: true,
            }
        );
    }

    #[test]
    fn test_unknown_transaction() {
        let mut ledger = build_ledger();
        let result = ledger.process(Transaction::Unknown {
            client_id: 5,
            tx_id: 10,
            amount: dec!(1.5),
        });
        assert_eq!(result, Err(Error::UnknownTransactionType));
    }

    // Helper functions to build transactions
    // Alternatively we could just create instances of Deposit, Withdrawal, etc
    // but we want to make sure we test the whole flow here, invoking the dynamic dispatch
    // through the Transaction enum
    fn deposit(client_id: u16, tx_id: u32, amount: Decimal) -> Transaction {
        Transaction::Deposit(Deposit {
            client_id,
            tx_id,
            amount,
        })
    }

    fn withdraw(client_id: u16, tx_id: u32, amount: Decimal) -> Transaction {
        Transaction::Withdrawal(Withdrawal {
            client_id,
            tx_id,
            amount,
        })
    }

    fn dispute(client_id: u16, tx_id: u32) -> Transaction {
        Transaction::Dispute(Dispute { client_id, tx_id })
    }

    fn resolve(client_id: u16, tx_id: u32) -> Transaction {
        Transaction::Resolve(Resolve { client_id, tx_id })
    }

    fn chargeback(client_id: u16, tx_id: u32) -> Transaction {
        Transaction::Chargeback(Chargeback { client_id, tx_id })
    }
}
