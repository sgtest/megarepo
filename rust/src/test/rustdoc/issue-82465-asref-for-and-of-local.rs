use std::convert::AsRef;
pub struct Local;

// @has issue_82465_asref_for_and_of_local/struct.Local.html '//h3[@class="code-header in-band"]' 'impl AsRef<str> for Local'
impl AsRef<str> for Local {
    fn as_ref(&self) -> &str {
        todo!()
    }
}

// @has - '//h3[@class="code-header in-band"]' 'impl AsRef<Local> for str'
impl AsRef<Local> for str {
    fn as_ref(&self) -> &Local {
        todo!()
    }
}
