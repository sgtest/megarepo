// Make sure #1399 stays fixed

fn main() {
    fn invoke(f: fn@()) { f(); }
    let k = ~22;
    let _u = {a: copy k};
    invoke(|| log(error, copy k) )
}
