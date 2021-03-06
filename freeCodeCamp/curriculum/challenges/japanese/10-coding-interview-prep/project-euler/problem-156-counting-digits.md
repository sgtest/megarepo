---
id: 5900f4091000cf542c50ff1b
title: '問題 156: 数字を数え上げる'
challengeType: 1
forumTopicId: 301787
dashedName: problem-156-counting-digits
---

# --description--

0 から始めて自然数を 10 進数で書くと、次のようになります。

0 1 2 3 4 5 6 7 8 9 10 11 12....

桁の数字 $d = 1$ について考えます。 それぞれの数 n を書いた後、それまでに出現した 1 の個数を更新します。この個数を $f(n, 1)$ とします。 最初のいくつかの $f(n, 1)$ の値は次のとおりです。

| $n$ | $f(n, 1)$ |
| --- | --------- |
| 0   | 0         |
| 1   | 1         |
| 2   | 1         |
| 3   | 1         |
| 4   | 1         |
| 5   | 1         |
| 6   | 1         |
| 7   | 1         |
| 8   | 1         |
| 9   | 1         |
| 10  | 2         |
| 11  | 4         |
| 12  | 5         |

$f(n, 1)$ が決して 3 にならないことに注目してください。

つまり、式 $f(n, 1) = n$ の最初の 2 つの解は $n = 0$ と $n = 1$ です。 その次の解は $n = 199981$ です。 同様に、関数 $f(n, d) は、$n$ が書かれた時点で桁の数字 d が出現した総数を導くものとします。

実のところ、$d ≠ 0$ のすべての数字 d について、式 $f(n, d) = n$ の最初の解は 0 です。 $f(n, d) = n$ の解の総和を $s(d)$ とします。

$s(1) = 22786974071$ が与えられます。 $1 ≤ d ≤ 9$ のとき、$\sum{s(d)}$ を求めなさい。

注: 一部の $n$ について、複数の $d$ の値に対して $f(n, d) = n$ となった場合、この $n$ 値は $f(n, d) = n$ である $d$ の値ごとに再びカウントされます。

# --hints--

`countingDigits()` は `21295121502550` を返す必要があります。

```js
assert.strictEqual(countingDigits(), 21295121502550);
```

# --seed--

## --seed-contents--

```js
function countingDigits() {

  return true;
}

countingDigits();
```

# --solutions--

```js
// solution required
```
