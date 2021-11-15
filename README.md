# transactions-rs

Process transactions in a csv file and output the final balances of each account.

## Usage

This project was build and tested on `Rust 1.56.1` (stable channel).

Output transaction errors to `stderr` and the final ledger to `stdout`:
```
cargo run [--release] -- <transaction csv file>
```

Dump errors and the final ledger to separate files:
```
cargo run [--release] -- <transaction csv file> 2> errors.txt > ledger.csv
```

The final account ledger will be printed to `stdout` when all transactions are processed. The errors for any
failed transactions will be printed to `stderr`.

## Assumptions Made

- Balances can be negative in the case that a client deposits money, withdraws it, and then a dispute is filed
- Transactions can go through the dispute process multiple times if a dispute is opened and then resolved
- Only deposits can be disputed, a withdrawal cannot (it didn't make much sense to me to have disputed withdrawals)
- When an account is locked, withdrawals and deposits are blocked, disputes/chargebacks/and dispute resolutions can still take place but the account will remain locked. This seemed correct, since we still want to do record keeping for past transactions when an account was locked
- We would not be tested on more than 4 digit decimal places
- If a line in the CSV file is invalid, that invalid line is just ignored and we move on to the next one
- Negative amounts are valid inputs for withdrawals or deposits, but the transaction doesn't go through (an error is raised)

## Design

The overall design is fairly straightforward, the CSV is streamed into `Transaction`'s using `serde` + `csv`,
those transactions are then 'processed' against a `Ledger`, updating account balances as needed and logging the transaction
for future record keeping.

### Libraries Used

- [serde](https://github.com/serde-rs/serde) was used for implementing the serialization/deserialization of structures
- [csv](https://github.com/BurntSushi/rust-csv) was used for reading and writing to the csv format
- [rust_decimal](https://github.com/paupino/rust-decimal) was used for decimal safety (see below)

### Decimal Safety

Since we are processing transactions, it's important that we use fixed precision in our numeric calculation instead of
floating precision numbers. Originally I planned ot just use a u64 with the lowest 4 digits representing the decimal values,
but to save time and use something a little more battle tested, I just opted for [rust_decimal](https://github.com/paupino/rust-decimal).

All currency amounts are translated into `Decimal` type with 4 digits for the decimal to make sure we don't run into any
errors resulting from using floating point numbers. If any transaction overflows or underflows a number, the program
will simply panic. This could later be changed to be handled as an error type with the transaction being declined instead.

### Type Safety

I added a bit of extra type safety by utilizing a `Transaction` enum (wrapping structs for each transaction type) to
prevent using fields in a transaction type that don't make sense. For example, `dispute`s don't really have a meaningful
amount attached to them, so the extra type safety prevents you from even touching that field on a `Dispute`.

Dynamic dispatch on transactions is implemented by having each transaction struct implement a `Process` trait, which
is a trait that does some sort of processing on a ledger. The `Transaction` enum then does a simple match to perform
the dynamic dispatch depending on which is the correct internal transaction type.

Originally I tried implementing deserialization for this type using `serde`'s [internally tagged enums](https://serde.rs/enum-representations.html#internally-tagged) feature.
Unfortunately that quickly turned into some pains, eventually realizing that internally tagged enums [aren't really supported](https://github.com/BurntSushi/rust-csv/issues/211#issuecomment-707620417)
by the csv crate. Thinking about it more, this makes sense though -- csv just isn't really a great format for shifting structures, and internally tagged structures work
best for data structures that change dramatically, not just adding/removing a field (`amount`). Due to this limitation,
I ended up needing to implement my own `Deserializer` for `serde` which uses an intermediate form to eventually reach the
proper type-safe form.

Looking back on this design and knowing what I learned, I probably would have opted to instead have a `Transaction` struct
that stores the `client_id`, `tx_id`, and `amount`, and a `type` enum. This would have simplified some things (especially needing
to build intermediate types for serde deserialization), at the cost of slightly reducing the type safety.

### Error Handling

Error handling is implemented throughout the transaction processing, with the `src/ledger/transaction.rs` file containing the
`Error` type for transactions. Whenever a transaction is processed, we return a `Result<Receipt>` which can either be an error,
or a receipt for the transaction. Since there isn't any sort of receipt we need right now, `type Receipt = ()`.

We handle these errors, some of which come from assumptions made about transaction processing:

```rust
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
}
```

Different errors are implemented for different types of things that can go wrong when processing a transaction. At the moment,
each error is caught in `main.rs` and then a message with the error is printed to `stderr`. That transaction is then ignored
for the remainder of the program. `stderr` is used to avoid polluting `stdout` for the account summary.

### Testing

For testing, I made use of both unit tests on each of my transaction types, along with some manual tests in the `test`
directory. The unit tests are located at the bottom of `src/ledger/transaction.rs` and attempt to cover a lot of the
error handling edge cases for each transaction type.

For more manual testing, I did a small basic test that was hand-written to try out a simple input, then I generated
a large csv filled with deposits, withdrawals, disputes, resolutions, and chargebacks. A python file `test/generate_large_disputes.py`
was created to generate the CSV and output what the expected result should be. This was then tested against the rust program
and the expected result was returned.

Finally, I generated a large multi-GB CSV file using `test/generate_large_csv.py` to make sure that the program can reasonably
handle large inputs without falling apart.

### Potential Improvements

#### Client Storage

Since our client set is so small, for input large data sets we could get some speed improvements by instead using an array
of pre-initialized clients with ids from `u16::MIN` to `u16::MAX`. The memory impact wouldn't be terrible, and if we initialize
the clients to be existing clients with empty balances, we can save needing to use an `Option`, eliminating a lot of extra branching
on each transaction (needing to check if a value exists in a map or is_some() for an `Option`).

#### Processing from concurrent streams

The current implementation breaks down quite a bit if we wanted to process the data coming in from multiple concurrent
streams. One obvious answer is to just wrap the entire `Ledger` in a `Mutex`, but that would be slow and requires us to
lock the entire ledger rather than just the client the transaction relates to.

It should be possible to have a `Mutex` for each `Account` and then have that `Account` keep track of the transactions
acted upon it. That way we can process transactions concurrently so long as the account isn't in use. This idea adds more
complexity if we needed to implement transactions that act on multiple accounts or even the entire ledger - lock order would become
extremely important as well to prevent a deadlock.

Another idea to consider is using `mpsc` to just push all our transactions into a sort of "transaction queue" and
then have a single transaction processor pulling from that transaction queue and processing the transactions in sequence.

Finally, we can also look into lock free data structures - it's not something I have a ton of experience with, and benchmarking
would be important because I know there are many cases where lock free structures can be slower than a structure with locks,
but it's worth considering.
