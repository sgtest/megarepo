enum hello = int;

fn main() {
    let hello = 0; //~ERROR declaration of `hello` shadows
}
