// run-rustfix

#![allow(unused)]
#![warn(clippy::redundant_async_block)]

async fn func1(n: usize) -> usize {
    n + 1
}

async fn func2() -> String {
    let s = String::from("some string");
    let f = async { (*s).to_owned() };
    let x = async { f.await };
    x.await
}

macro_rules! await_in_macro {
    ($e:expr) => {
        std::convert::identity($e).await
    };
}

async fn func3(n: usize) -> usize {
    // Do not lint (suggestion would be `std::convert::identity(func1(n))`
    // which copies code from inside the macro)
    async move { await_in_macro!(func1(n)) }.await
}

// This macro should never be linted as `$e` might contain `.await`
macro_rules! async_await_parameter_in_macro {
    ($e:expr) => {
        async { $e.await }
    };
}

// MISSED OPPORTUNITY: this macro could be linted as the `async` block does not
// contain code coming from the parameters
macro_rules! async_await_in_macro {
    ($f:expr) => {
        ($f)(async { func2().await })
    };
}

fn main() {
    let fut1 = async { 17 };
    let fut2 = async { fut1.await };

    let fut1 = async { 25 };
    let fut2 = async move { fut1.await };

    let fut = async { async { 42 }.await };

    // Do not lint: not a single expression
    let fut = async {
        func1(10).await;
        func2().await
    };

    // Do not lint: expression contains `.await`
    let fut = async { func1(func2().await.len()).await };

    let fut = async_await_parameter_in_macro!(func2());
    let fut = async_await_in_macro!(std::convert::identity);
}
