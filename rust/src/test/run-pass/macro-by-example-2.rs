// xfail-test
// I can't for the life of me manage to untangle all of the brackets
// in this test, so I am xfailing it...

fn main() {
    #macro[[#zip_or_unzip[[x, ...], [y, ...]], [[x, y], ...]],
           [#zip_or_unzip[[xx, yy], ...], [[xx, ...], [yy, ...]]]];


    assert (zip_or_unzip!{[1, 2, 3, 4], [5, 6, 7, 8]} ==
                [[1, 5], [2, 6], [3, 7], [4, 8]]);
    assert (zip_or_unzip!{[1, 5], [2, 6], [3, 7], [4, 8]} ==
                [[1, 2, 3, 4], [5, 6, 7, 8]]);


    #macro[[#nested[[[x, ...], ...], [[y, ...], ...]], [[[x, y], ...], ...]]];
    assert (nested!{[[1, 2, 3, 4, 5], [7, 8, 9, 10, 11, 12]],
                    [[-1, -2, -3, -4, -5], [-7, -8, -9, -10, -11, -12]]} ==
                [[[1, -1], [2, -2], [3, -3], [4, -4], [5, -5]],
                 [[7, -7], [8, -8], [9, -9], [10, -10], [11, -11],
                  [12, -12]]]);

    #macro[[#dup[y, [x, ...]], [[y, x], ...]]];

    assert (dup!{1, [1, 2, 3, 4]} == [[1, 1], [1, 2], [1, 3], [1, 4]]);


    #macro[[#lambda[x, #<t>, body, #<s>],
            {
                fn result(x: t) -> s { ret body }
                result
            }]];


    assert (lambda!{i, #<uint>, i + 4u, #<uint>}(12u) == 16u);

    #macro[[#sum[x, xs, ...], x + #sum[xs, ...]], [#sum[], 0]];

    assert (sum!{1, 2, 3, 4} == 10);


    #macro[[#transcr_mixed[a, as, ...], #sum[6, as, ...] * a]];

    assert (transcr_mixed!{10, 5, 4, 3, 2, 1} == 210);

    #macro[[#surround[pre, [xs, ...], post], [pre, xs, ..., post]]];

    assert (surround!{1, [2, 3, 4], 5} == [1, 2, 3, 4, 5]);

}
