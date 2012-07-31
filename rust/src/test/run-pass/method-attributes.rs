// pp-exact - Make sure we print all the attributes

#[frobable]
trait frobable {
    #[frob_attr]
    fn frob();
    #[defrob_attr]
    fn defrob();
}

#[int_frobable]
impl frobable of frobable for int {
    #[frob_attr1]
    fn frob() {
        #[frob_attr2];
    }

    #[defrob_attr1]
    fn defrob() {
        #[defrob_attr2];
    }
}

fn main() { }
