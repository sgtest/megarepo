#![crate_type = "lib"]
#![feature(label_break_value)]

fn lbv_macro_test_hygiene_respected() {
    macro_rules! mac2 {
        ($val:expr) => {
            break 'a $val; //~ ERROR undeclared label `'a` [E0426]
        };
    }
    let x: u8 = 'a: {
        'b: {
            if true {
                mac2!(2);
            }
        };
        0
    };
    assert_eq!(x, 2);

    macro_rules! mac3 {
        ($val:expr) => {
            'a: {
            //~^ WARNING `'a` shadows a label
            //~| WARNING `'a` shadows a label
            //~| WARNING `'a` shadows a label
                $val
            }
        };
    }
    let x: u8 = mac3!('b: { //~ WARNING `'b` shadows a label
        if true {
            break 'a 3; //~ ERROR undeclared label `'a` [E0426]
        }
        0
    });
    assert_eq!(x, 3);
    let x: u8 = mac3!(break 'a 4); //~ ERROR undeclared label `'a` [E0426]
    assert_eq!(x, 4);
}
