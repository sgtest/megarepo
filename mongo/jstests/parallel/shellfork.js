import {fork, Thread} from "jstests/libs/parallelTester.js";

let a = fork(function(a, b) {
    return a / b;
}, 10, 2);
a.start();
let b = fork(function(a, b, c) {
    return a + b + c;
}, 18, " is a ", "multiple of 3");
let makeFunny = function(text) {
    return text + " ha ha!";
};
let c = fork(makeFunny, "paisley");
c.start();
b.start();
b.join();
assert.eq(5, a.returnData());
assert.eq("18 is a multiple of 3", b.returnData());
assert.eq("paisley ha ha!", c.returnData());

let z = fork(async function(a) {
    const {fork} = await import("jstests/libs/parallelTester.js");
    var y = fork(function(a) {
        return a + 1;
    }, 5);
    y.start();
    return y.returnData() + a;
}, 1);
z.start();
assert.eq(7, z.returnData());

let t = 1;
z = new Thread(function() {
    assert(typeof (t) == "undefined", "t not undefined");
    t = 5;
    return t;
});
z.start();
assert.eq(5, z.returnData());
assert.eq(1, t);
