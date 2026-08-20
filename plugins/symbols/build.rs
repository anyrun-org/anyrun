use std::{
    env,
    fs::{self, File},
    io::Write,
};

fn main() {
    let string = fs::read_to_string("res/UnicodeData.txt").expect("Failed to load unicode data!");
    let mut file = File::create(format!("{}/unicode.rs", env::var("OUT_DIR").unwrap()))
        .expect("Unable to create unicode output file!");

    file.write_all(b"const UNICODE_CHARS: &[(&str, &str)] = &[\n")
        .unwrap();
    string.lines().for_each(|line| {
        let fields = line.split(';').collect::<Vec<_>>();
        if fields[1] != "<control>"
            && u32::from_str_radix(fields[0], 16)
                .ok()
                .and_then(char::from_u32)
                .is_some()
        {
            writeln!(file, "(r#\"{}\"#, \"\\u{{{}}}\"),", fields[1], fields[0]).unwrap();
        }
    });

    file.write_all(b"];\n").unwrap();
}
