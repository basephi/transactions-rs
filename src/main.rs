use std::process::exit;
use std::{env, io};

use crate::ledger::{Ledger, Transaction};

mod ledger;

fn main() {
    let args: Vec<_> = env::args().collect();
    if args.len() != 2 {
        print_usage();
        exit(1);
    }

    let mut ledger = Ledger::new();

    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(&args[1])
        .expect("a file");
    for result in rdr.deserialize() {
        let tx: Transaction = result.expect("expected a csv of transactions");
        match ledger.process(tx) {
            Ok(_) => {}
            Err(err) => {
                eprintln!("{} failed: {}", tx, err)
            }
        }
    }
    eprintln!("Done processing!");

    let mut wtr = csv::Writer::from_writer(io::stdout());
    for account in ledger.accounts() {
        wtr.serialize(account).unwrap();
    }
    wtr.flush().unwrap();
}

fn print_usage() {
    println!("Usage: transactions-rs <csv file>");
    println!();
    println!("The file must be a valid csv with the columns type,client,tx,amount");
}
