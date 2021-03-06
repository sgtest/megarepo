---
id: 5900f50c1000cf542c51001e
title: 'Завдання 415: Титанічні множини'
challengeType: 1
forumTopicId: 302084
dashedName: problem-415-titanic-sets
---

# --description--

Набір точок ґратки $S$ називається титанічною множиною, якщо існує лінія, що проходить рівно через дві точки в $S$.

Прикладом титанічної множини є $S = \\{(0, 0), (0, 1), (0, 2), (1, 1), (2, 0), (1, 0), (1, 0)\\}$, де лінія, що проходить через (0, 1) та (2, 0) не проходить через будь-які інші точки в $S$.

З іншого боку, множина {(0, 0), (1, 1), (2, 2), (4, 4)} не є титанічною множиною, оскільки пряма, що проходить через дві точки у множині, також проходить через дві інші.

Для будь-якого додатного цілого числа $N$, нехай $T(N)$ — кількість титанічних множин $S$, кожна точка яких ($x$ $y$) задовільняє $0 ≤ x$, $y ≤ N$. Можна перевірити, що $T(1) = 11$, $T(2) = 494$, $T(4) = 33\\,554\\,178$, $T(111)\bmod {10}^8 = 13\\,500\\,401$ і $T({10}^5)\bmod {10}^8 = 63\\,259\\,062$.

Знайдіть $T({10}^{11})\bmod {10}^8$.

# --hints--

`titanicSets()` має повернути `55859742`.

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
