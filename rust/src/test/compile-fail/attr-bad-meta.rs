// error-pattern:expected `]`

// asterisk is bogus
#[attr*]
mod m {
    #[legacy_exports]; }
