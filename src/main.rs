use std::io;

use crate::database::Database;

mod binder;
mod buffer;
mod catalog;
mod common;
mod database;
mod dbtype;
mod execution;
mod optimizer;
mod parser;
mod planner;
mod storage;

fn main() {
    println!(":) Welcome to the bustubx, please input sql.");

    let mut db = Database::new_on_disk("test.db");
    loop {
        println!("> ");
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                db.run(&input);
            }
            Err(_) => {
                println!("Error reading from stdin");
                continue;
            }
        }
    }
}
