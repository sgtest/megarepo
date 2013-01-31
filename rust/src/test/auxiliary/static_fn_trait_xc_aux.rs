pub mod num {
    pub trait Num2 {
        static pure fn from_int2(n: int) -> Self;
    }
}

pub mod float {
    impl float: ::num::Num2 {
        static pure fn from_int2(n: int) -> float { return n as float;  }
    }
}
