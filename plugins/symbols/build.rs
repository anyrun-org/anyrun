use std::{
    env,
    fs::{self, File},
    io::Write,
};

fn main() {
    let string = fs::read_to_string("res/UnicodeData.txt").expect("Failed to load unicode data!");
    let mut file = File::create(format!("{}/unicode.rs", env::var("OUT_DIR").unwrap()))
        .expect("Unable to create unicode output file!");

    file.write_all(b"#[allow(text_direction_codepoint_in_literal)]\nconst UNICODE_CHARS: &[(&str, &str)] = &[\n")
        .unwrap();
    string.lines().for_each(|line| {
        let fields = line.split(';').collect::<Vec<_>>();
        let chr = match char::from_u32(u32::from_str_radix(fields[0], 16).unwrap()) {
            Some(char) => char,
            None => return,
        };

        if fields[1] != "<control>" {
            file.write_all(format!("(r#\"{}\"#, r#\"{}\"#),\n", fields[1], chr).as_bytes())
                .unwrap();
        }
    });

    file.write_all(b"];\n").unwrap();
}
