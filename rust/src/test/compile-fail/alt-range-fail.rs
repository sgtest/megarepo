//error-pattern: lower range bound
//error-pattern: non-numeric
//error-pattern: mismatched types

fn main() {
    alt 5u {
      6u to 1u => { }
      _ => { }
    };

    alt "wow" {
      "bar" to "foo" => { }
    };

    alt 5u {
      'c' to 100u => { }
      _ => { }
    };
}
