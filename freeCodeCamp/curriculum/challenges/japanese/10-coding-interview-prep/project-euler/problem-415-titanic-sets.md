---
id: 5900f50c1000cf542c51001e
title: '問題 415: タイタニック集合'
challengeType: 1
forumTopicId: 302084
dashedName: problem-415-titanic-sets
---

# --description--

格子点の集合 $S$ に含まれる格子点のうちちょうど 2 点を通る直線がある場合、その格子点の集合 $S$ をタイタニック集合と呼びます。

タイタニック集合の例を挙げます。$S = \\{(0, 0), (0, 1), (0, 2), (1, 1), (2, 0), (1, 0)\\}$ ここで、(0, 1) と (2, 0) を通る線は $S$ の他のいずれの点も通りません。

一方、集合 {(0, 0), (1, 1), (2, 2), (4, 4)} はタイタニック集合ではありません。なぜなら、集合内のいずれの 2 点を通る線も、さらに他の 2 点を通るからです。

任意の正の整数 $N$ について、すべての点 ($x$, $y$) が $0 ≤ x$, $y ≤ N$ を満たすようなタイタニック集合 $S$ の数を $T(N)$ とします。 $T(1) = 11$, $T(2) = 494$, $T(4) = 33\\,554\\,178$, $T(111)\bmod {10}^8 = 13\\,500\\,401$, $T({10}^5)\bmod {10}^8 = 63\\,259\\,062$ であることを確認できます。

$T({10}^{11})\bmod {10}^8$ を求めなさい。

# --hints--

`titanicSets()` は `55859742` を返す必要があります。

```js
assert.strictEqual(titanicSets(), 55859742);
```

# --seed--

## --seed-contents--

```js
function titanicSets() {

  return true;
}

titanicSets();
```

# --solutions--

```js
// solution required
```
