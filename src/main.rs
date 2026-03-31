use std::io::{self, Read, Write};

fn main() {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .expect("failed to read stdin");

    match docx2txt::convert(&input) {
        Ok(text) => io::stdout().write_all(text.as_bytes()).unwrap(),
        Err(e) => {
            eprintln!("docx2txt: {}", e);
            std::process::exit(1);
        }
    }
}
