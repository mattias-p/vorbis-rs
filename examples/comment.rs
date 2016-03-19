extern crate vorbis_sys;
extern crate vorbis;

use std::error;
use std::fs::File;
use std::path::Path;

fn main() {
    // let path = Path::new("examples/Epoq-Lepidoptera.ogg");
    let path = Path::new("examples/thesong.ogg");
    let display = path.display();
    let file = match File::open(&path) {
        Err(why) => {
            panic!("Couldn't open {}: {}",
                   display,
                   error::Error::description(&why))
        }   
        Ok(file) => file,
    };

    let decoder = vorbis::Decoder::new(file).unwrap();
    println!("{}", decoder.vendor().expect("vendor"));
    for item in decoder.comments() {
        match item {
            Ok((key, value)) => println!("{} = {}", key, value),
            Err(err) => println!("{}", err),
        }
    }
}
