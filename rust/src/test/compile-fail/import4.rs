// error-pattern: import

mod a { import foo = b::foo; export foo; }
mod b { import foo = a::foo; export foo; }

fn main(args: ~[str]) { debug!{"loop"}; }
