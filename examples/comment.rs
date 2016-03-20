extern crate vorbis_sys;
extern crate vorbis;

use std::error;
use std::fs::File;
use std::path::Path;
use std::io::{Read, Write, Seek};

#[derive(Debug)]
enum MyError {
    Vorbis(vorbis::VorbisError),
    ParseInt(std::num::ParseIntError),
}

impl std::error::Error for MyError {
    fn description(&self) -> &str {
        match self {
            &MyError::ParseInt(_) => "A string could not be parsed as an integer",
            &MyError::Vorbis(_) => "An error occurred in the Vorbis decoder",
        }
    }

    fn cause(&self) -> Option<&std::error::Error> {
        match self {
            &MyError::ParseInt(ref err) => Some(err as &std::error::Error),
            &MyError::Vorbis(ref err) => Some(err as &std::error::Error),
        }
    }
}

impl std::fmt::Display for MyError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(fmt, "{}", std::error::Error::description(self))
    }
}

impl From<vorbis::VorbisError> for MyError {
    fn from(err: vorbis::VorbisError) -> MyError {
        MyError::Vorbis(err)
    }
}

impl From<std::num::ParseIntError> for MyError {
    fn from(err: std::num::ParseIntError) -> MyError {
        MyError::ParseInt(err)
    }
}

fn warn(message: &str) {
    writeln!(&mut std::io::stderr(), "Warning: {}", message).unwrap();
}

fn get_splice<R>(decoder: &vorbis::Decoder<R>) -> Result<Option<u64>, MyError>
    where R: Read + Seek
{
    let values: Vec<String> = try!(decoder.get_comment("SPLICEPOINT"));
    values.iter().fold(Ok(None), |acc, value| {
        let acc = acc.unwrap();
        let value: u64 = try!(value.parse::<u64>());
        let min = acc.map_or(value, |acc_value| std::cmp::min(acc_value, value));
        Ok(Some(min))
    })
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

    let splice = get_splice(&decoder);

    if let Some(splice) = splice.expect("SPLICEPOINT") {
        println!("SPLICEPOINT: {}", splice);
    } else {
        warn("No SPLICEPOINT found.");
    }
}
