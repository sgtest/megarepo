// The Computer Language Benchmarks Game
// http://benchmarksgame.alioth.debian.org/
//
// contributed by the Rust Project Developers

// Copyright (c) 2012-2014 The Rust Project Developers
//
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
//
// - Redistributions of source code must retain the above copyright
//   notice, this list of conditions and the following disclaimer.
//
// - Redistributions in binary form must reproduce the above copyright
//   notice, this list of conditions and the following disclaimer in
//   the documentation and/or other materials provided with the
//   distribution.
//
// - Neither the name of "The Computer Language Benchmarks Game" nor
//   the name of "The Computer Language Shootout Benchmarks" nor the
//   names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior
//   written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
// FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
// COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
// INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
// (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT,
// STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED
// OF THE POSSIBILITY OF SUCH DAMAGE.

// no-pretty-expanded

use self::Color::{Red, Yellow, Blue};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::fmt;
use std::thread;

fn print_complements() {
    let all = [Blue, Red, Yellow];
    for aa in &all {
        for bb in &all {
            println!("{:?} + {:?} -> {:?}", *aa, *bb, transform(*aa, *bb));
        }
    }
}

#[derive(Copy)]
enum Color {
    Red,
    Yellow,
    Blue,
}

impl fmt::Debug for Color {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let str = match *self {
            Red => "red",
            Yellow => "yellow",
            Blue => "blue",
        };
        write!(f, "{}", str)
    }
}

#[derive(Copy)]
struct CreatureInfo {
    name: usize,
    color: Color
}

fn show_color_list(set: Vec<Color>) -> String {
    let mut out = String::new();
    for col in &set {
        out.push(' ');
        out.push_str(&format!("{:?}", col));
    }
    out
}

fn show_digit(nn: usize) -> &'static str {
    match nn {
        0 => {" zero"}
        1 => {" one"}
        2 => {" two"}
        3 => {" three"}
        4 => {" four"}
        5 => {" five"}
        6 => {" six"}
        7 => {" seven"}
        8 => {" eight"}
        9 => {" nine"}
        _ => {panic!("expected digits from 0 to 9...")}
    }
}

struct Number(usize);
impl fmt::Debug for Number {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut out = vec![];
        let Number(mut num) = *self;
        if num == 0 { out.push(show_digit(0)) };

        while num != 0 {
            let dig = num % 10;
            num = num / 10;
            let s = show_digit(dig);
            out.push(s);
        }

        for s in out.iter().rev() {
            try!(write!(f, "{}", s))
        }
        Ok(())
    }
}

fn transform(aa: Color, bb: Color) -> Color {
    match (aa, bb) {
        (Red,    Red   ) => { Red    }
        (Red,    Yellow) => { Blue   }
        (Red,    Blue  ) => { Yellow }
        (Yellow, Red   ) => { Blue   }
        (Yellow, Yellow) => { Yellow }
        (Yellow, Blue  ) => { Red    }
        (Blue,   Red   ) => { Yellow }
        (Blue,   Yellow) => { Red    }
        (Blue,   Blue  ) => { Blue   }
    }
}

fn creature(
    name: usize,
    mut color: Color,
    from_rendezvous: Receiver<CreatureInfo>,
    to_rendezvous: Sender<CreatureInfo>,
    to_rendezvous_log: Sender<String>
) {
    let mut creatures_met = 0;
    let mut evil_clones_met = 0;
    let mut rendezvous = from_rendezvous.iter();

    loop {
        // ask for a pairing
        to_rendezvous.send(CreatureInfo {name: name, color: color}).unwrap();

        // log and change, or quit
        match rendezvous.next() {
            Some(other_creature) => {
                color = transform(color, other_creature.color);

                // track some statistics
                creatures_met += 1;
                if other_creature.name == name {
                   evil_clones_met += 1;
                }
            }
            None => break
        }
    }
    // log creatures met and evil clones of self
    let report = format!("{}{:?}", creatures_met, Number(evil_clones_met));
    to_rendezvous_log.send(report).unwrap();
}

fn rendezvous(nn: usize, set: Vec<Color>) {
    // these ports will allow us to hear from the creatures
    let (to_rendezvous, from_creatures) = channel::<CreatureInfo>();

    // these channels will be passed to the creatures so they can talk to us
    let (to_rendezvous_log, from_creatures_log) = channel::<String>();

    // these channels will allow us to talk to each creature by 'name'/index
    let to_creature: Vec<Sender<CreatureInfo>> =
        set.iter().enumerate().map(|(ii, &col)| {
            // create each creature as a listener with a port, and
            // give us a channel to talk to each
            let to_rendezvous = to_rendezvous.clone();
            let to_rendezvous_log = to_rendezvous_log.clone();
            let (to_creature, from_rendezvous) = channel();
            thread::spawn(move|| {
                creature(ii,
                         col,
                         from_rendezvous,
                         to_rendezvous,
                         to_rendezvous_log);
            });
            to_creature
        }).collect();

    let mut creatures_met = 0;

    // set up meetings...
    for _ in 0..nn {
        let fst_creature = from_creatures.recv().unwrap();
        let snd_creature = from_creatures.recv().unwrap();

        creatures_met += 2;

        to_creature[fst_creature.name].send(snd_creature).unwrap();
        to_creature[snd_creature.name].send(fst_creature).unwrap();
    }

    // tell each creature to stop
    drop(to_creature);

    // print each color in the set
    println!("{}", show_color_list(set));

    // print each creature's stats
    drop(to_rendezvous_log);
    for rep in from_creatures_log.iter() {
        println!("{}", rep);
    }

    // print the total number of creatures met
    println!("{:?}\n", Number(creatures_met));
}

fn main() {
    let nn = if std::env::var_os("RUST_BENCH").is_some() {
        200000
    } else {
        std::env::args()
                       .nth(1)
                       .and_then(|arg| arg.parse().ok())
                       .unwrap_or(600)
    };

    print_complements();
    println!("");

    rendezvous(nn, vec!(Blue, Red, Yellow));

    rendezvous(nn,
        vec!(Blue, Red, Yellow, Red, Yellow, Blue, Red, Yellow, Red, Blue));
}
