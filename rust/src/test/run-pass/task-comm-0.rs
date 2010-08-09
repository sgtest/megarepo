io fn main() -> () {
    test05();
}

io fn test05_start(chan[int] ch) {
    ch <| 10;
    ch <| 20;
    ch <| 30;
}

io fn test05() {
    let port[int] po = port();
    let chan[int] ch = chan(po);
    spawn test05_start(chan(po));
    let int value <- po;
    value <- po;
    value <- po;
    check(value == 30);
}
