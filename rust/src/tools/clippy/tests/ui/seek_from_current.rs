// run-rustfix
#![warn(clippy::seek_from_current)]
#![feature(custom_inner_attributes)]

use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};

fn _msrv_1_50() -> io::Result<()> {
    #![clippy::msrv = "1.50"]
    let mut f = File::create("foo.txt")?;
    f.write_all(b"Hi!")?;
    f.seek(SeekFrom::Current(0))?;
    f.seek(SeekFrom::Current(1))?;
    Ok(())
}

fn _msrv_1_51() -> io::Result<()> {
    #![clippy::msrv = "1.51"]
    let mut f = File::create("foo.txt")?;
    f.write_all(b"Hi!")?;
    f.seek(SeekFrom::Current(0))?;
    f.seek(SeekFrom::Current(1))?;
    Ok(())
}

fn main() {}
