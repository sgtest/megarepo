tag maybe_pointy {
    none;
    p(@pointy);
}

type pointy = {
    mutable a : maybe_pointy,
    d : sendfn() -> uint,
};

fn make_uniq_closure<A:send>(a: A) -> sendfn() -> uint {
    sendfn() -> uint { ptr::addr_of(a) as uint }
}

fn empty_pointy() -> @pointy {
    ret @{
        mutable a : none,
        d : make_uniq_closure("hi")
    }
}

fn main()
{
    let v = empty_pointy();
    v.a = p(v);
}
