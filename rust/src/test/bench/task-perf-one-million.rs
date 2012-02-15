// Test for concurrent tasks

enum msg {
    ready(comm::chan<msg>),
    start,
    done(int),
}

fn calc(children: uint, parent_ch: comm::chan<msg>) {
    let port = comm::port();
    let chan = comm::chan(port);
    let child_chs = [];
    let sum = 0;

    iter::repeat (children) {||
        task::spawn {||
            calc(0u, chan);
        };
    }

    iter::repeat (children) {||
        alt check comm::recv(port) {
          ready(child_ch) {
            child_chs += [child_ch];
          }
        }
    }

    comm::send(parent_ch, ready(chan));

    alt check comm::recv(port) {
        start {
          vec::iter (child_chs) { |child_ch|
              comm::send(child_ch, start);
          }
        }
    }

    iter::repeat (children) {||
        alt check comm::recv(port) {
          done(child_sum) { sum += child_sum; }
        }
    }

    comm::send(parent_ch, done(sum + 1));
}

fn main(args: [str]) {
    let children = if vec::len(args) == 2u {
        uint::from_str(args[1])
    } else {
        100u
    };
    let port = comm::port();
    let chan = comm::chan(port);
    task::spawn {||
        calc(children, chan);
    };
    alt check comm::recv(port) {
      ready(chan) {
        comm::send(chan, start);
      }
    }
    let sum = alt check comm::recv(port) {
      done(sum) { sum }
    };
    #error("How many tasks? %d tasks.", sum);
}