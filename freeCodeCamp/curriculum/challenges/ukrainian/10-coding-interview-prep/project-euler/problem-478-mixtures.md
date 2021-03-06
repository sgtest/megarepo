---
id: 5900f54c1000cf542c51005e
title: 'Завдання 478: Комбінації'
challengeType: 1
forumTopicId: 302155
dashedName: problem-478-mixtures
---

# --description--

Розгляньмо комбінації трьох речовин: $A$, $B$ та $C$. Комбінації можна описати за співвідношенням кількості $A$, $B$, та $C$ в ній, тобто, $(a : b : c)$. Наприклад, комбінація, що описана співвідношенням (2 : 3 : 5), містить 20% $A$, 30% $B$ та 50% $C$.

В контексті цієї проблеми ми не можемо відокремити окремі компоненти із комбінації. Однак ми можемо комбінувати різні кількості різних комбінацій, щоб утворювати комбінації з новими співвідношеннями.

Наприклад, скажімо, що у нас є три комбінації зі співвідношеннями (3 : 0 : 2), (3 : 6 : 11) та (3 : 3 : 4). Змішуючи 10 одиниць першої, 20 одиниць другої та 30 одиниць третьої, ми отримуємо нову комбінацію зі співвідношенням (6 : 5 : 9), оскільки: ($10 \times \frac{3}{5} + 20 \times \frac{3}{20} + 30 \times \frac{3}{10}$ : $10 \times \frac{0}{5} + 20 \times \frac{6}{20} + 30 \times \frac{3}{10}$ : $10 \times \frac{2}{5} + 20 \times \frac{11}{20} + 30 \times \frac{4}{10}$) = (18 : 15 : 27) = (6 : 5 : 9)

Однак з трьома однаковими комбінаціями неможливо сформувати співвідношення (3 : 2 : 1), оскільки кількість $B$ завжди менша за кількість $C$.

Нехай $n$ буде позитивним цілим числом. Припустимо, що для кожної трійки чисел $(a, b, c)$ з $0 ≤ a, b, c ≤ n$ та $gcd(a, b, c) = 1$, ми маємо комбінацію зі співвідношенням $(a : b : c)$. Нехай $M(n)$ - це множина всіх таких комбінацій.

Наприклад, $M(2)$ містить 19 комбінацій з наступними співвідношеннями:

{(0 : 0 : 1), (0 : 1 : 0), (0 : 1 : 1), (0 : 1 : 2), (0 : 2 : 1), (1 : 0 : 0), (1 : 0 : 1), (1 : 0 : 2), (1 : 1 : 0), (1 : 1 : 1), (1 : 1 : 2), (1 : 2 : 0), (1 : 2 : 1), (1 : 2 : 2), (2 : 0 : 1), (2 : 1 : 0), (2 : 1 : 1), (2 : 1 : 2), (2 : 2 : 1)}.

Нехай $E(n)$ - це кількість підмножин $M(n)$, які можуть створити комбінацію зі співвідношенням (1 : 1 : 1), тобто комбінацію з рівними частинами $A$, $B$ та $C$.

Ми можемо перевірити, що $E(1) = 103$, $E(2) = 520\\,447$, $E(10)\bmod {11}^8 = 82\\,608\\,406$ and $E(500)\bmod {11}^8 = 13\\,801\\,403$.

Знайдіть $E(10\\,000\\,000)\bmod {11}^8$.

# --hints--

`mixtures()` має повернути `59510340`.

```js
assert.strictEqual(mixtures(), 59510340);
```

# --seed--

## --seed-contents--

```js
function mixtures() {

  return true;
}

mixtures();
```

# --solutions--

```js
// solution required
```
