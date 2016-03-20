extern crate vorbis_sys;
extern crate vorbis;

use std::error;
use std::fs::File;
use std::path::Path;
use std::io::Write;
use std::error::Error;

fn warn(message: &str) {
    writeln!(&mut std::io::stderr(), "Warning: {}", message).unwrap();
}

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

    let splice = decoder.comments()
                        .flat_map(|result| {
                            result.map_err(|err| warn(err.description()))
                                  .into_iter()
                        })
                        .flat_map(|key_value| {
                            if key_value.0 == "SPLICE" {
                                Some(key_value.1)
                            } else {
                                None
                            }
                        })
                        .fold(None, |acc, value| {
                            if acc.is_some() {
                                warn("Multiple SPLICE comments encountered, using the last one.");
                            }
                            Some(value)
                        });

    if let Some(splice) = splice {
        println!("SPLICE: {}", splice);
    } else {
        warn("No SPLICE Vorbis comment found.");
    }
}
