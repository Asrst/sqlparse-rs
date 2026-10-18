//! Bench harness: reads SQL from stdin, tokenizes once, reports token count
//! (stdout) and elapsed time (stderr). Token count is the equal-work check
//! against the Python baseline.

use std::io::Read;

fn main() {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("stdin must be valid UTF-8");
    let start = std::time::Instant::now();
    let tokens = sqlparse_core::tokenize(&input);
    let elapsed = start.elapsed();
    eprintln!(
        "sqlparse-rs: {} tokens in {:?} ({:.1} MB/s)",
        tokens.len(),
        elapsed,
        input.len() as f64 / 1024.0 / 1024.0 / elapsed.as_secs_f64()
    );
    println!("{}", tokens.len());
}
