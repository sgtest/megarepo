// This test will call __morestack with various minimum stack sizes

extern mod std;

fn getbig(&&i: int) {
    if i != 0 {
        getbig(i - 1);
    }
}

fn main() {
    let mut sz = 400u;
    while sz < 500u {
        task::try(|| getbig(200) );
        sz += 1u;
    }
}