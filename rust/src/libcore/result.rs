/*
Module: result

A type representing either success or failure
*/

/* Section: Types */

/*
Tag: t

The result type
*/
tag t<T, U> {
    /*
    Variant: ok

    Contains the result value
    */
    ok(T);
    /*
    Variant: err

    Contains the error value
    */
    err(U);
}

/* Section: Operations */

/*
Function: get

Get the value out of a successful result

Failure:

If the result is an error
*/
fn get<T, U>(res: t<T, U>) -> T {
    alt res {
      ok(t) { t }
      err(_) {
        // FIXME: Serialize the error value
        // and include it in the fail message
        fail "get called on error result";
      }
    }
}

/*
Function: get_err

Get the value out of an error result

Failure:

If the result is not an error
*/
fn get_err<T, U>(res: t<T, U>) -> U {
    alt res {
      err(u) { u }
      ok(_) {
        fail "get_error called on ok result";
      }
    }
}

/*
Function: success

Returns true if the result is <ok>
*/
fn success<T, U>(res: t<T, U>) -> bool {
    alt res {
      ok(_) { true }
      err(_) { false }
    }
}

/*
Function: failure

Returns true if the result is <error>
*/
fn failure<T, U>(res: t<T, U>) -> bool {
    !success(res)
}

/*
Function: chain

Call a function based on a previous result

If `res` is <ok> then the value is extracted and passed to `op` whereupon
`op`s result is returned. if `res` is <err> then it is immediately returned.
This function can be used to compose the results of two functions.

Example:

> let res = chain(read_file(file), { |buf|
>   ok(parse_buf(buf))
> })

*/
fn chain<T, copy U, copy V>(res: t<T, V>, op: block(T) -> t<U, V>)
    -> t<U, V> {
    alt res {
      ok(t) { op(t) }
      err(e) { err(e) }
    }
}
