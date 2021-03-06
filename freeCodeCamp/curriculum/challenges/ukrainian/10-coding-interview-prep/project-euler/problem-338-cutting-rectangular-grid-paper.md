---
id: 5900f4be1000cf542c50ffd1
title: 'Задача 338: Вирізання прямокутної сітчастої сітки'
challengeType: 1
forumTopicId: 301996
dashedName: problem-338-cutting-rectangular-grid-paper
---

# --description--

Дано прямокутний аркуш паперу в решітку з цілочисельними розмірами $w$ × $h$. Відстань між ґратками дорівнює 1.

Якщо розрізати аркуш по лініях ґратки на дві частини та переставити їх так, щоб вони не перекривали одне одного, можна отримати ще один прямокутник з іншими розмірами.

До прикладу, розрізавши та переставивши частинки аркуша паперу з розмірами 9 × 4 так, як показано нижче, можна зробити прямокутник зі сторонами 18 × 2, 12 × 3 та 6 × 6:

<img class="img-responsive center-block" alt="з аркуша розміром 9 x 4, який розділили на три частини, можна одержати прямокутники зі сторонами 18 x 2, 12 x 3 та 6 x 6" src="https://cdn.freecodecamp.org/curriculum/project-euler/cutting-rectangular-grid-paper.gif" style="background-color: white; padding: 10px;" />

Схожим чином із листка 9 × 8 вийде прямокутник на 18 × 4 та 12 × 6.

Для пари $w$ та $h$, нехай $F(w, h)$ — та кількість прямокутників, яку можна отримати з аркуша паперу розміром $w$ × $h$. Наприклад, $F(2, 1) = 0$, $F(2, 2) = 1$, $F(9, 4) = 3$ та $F(9, 8) = 2$. Зверніть увагу на те, що прямокутники, що збігаються з вихідним, не враховуються в $F(w, h)$. До того ж прямокутники з розмірами $w$ × $h$ та $h$ × $w$ — вважаються однаковими.

Для цілого числа $N$, нехай $G(N)$ — це сума $F(w, h)$ для всіх пар $w$ та $h$, яка задовольняє умову $0 &lt; h ≤ w ≤ N$. Можна перевірити, що $G(10) = 55$, $G({10}^3) = 971\\,745$ та $G({10}^5) = 9\\,992\\,617\\,687$.

Знайдіть $G({10}^{12})$. Надайте відповідь за модулем ${10}^8$.

# --hints--

`cuttingRectangularGridPaper()` має повернути `15614292`.

```js
assert.strictEqual(cuttingRectangularGridPaper(), 15614292);
```

# --seed--

## --seed-contents--

```js
function cuttingRectangularGridPaper() {

  return true;
}

cuttingRectangularGridPaper();
```

# --solutions--

```js
// solution required
```
