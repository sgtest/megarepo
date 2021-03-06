---
id: 5900f4a71000cf542c50ffba
title: 'Проблема 315: Годинники коренів чисел'
challengeType: 1
forumTopicId: 301971
dashedName: problem-315-digital-root-clocks
---

# --description--

<img class="img-responsive center-block" alt="анімація годинників Сема і Макса, що обчислюють цифрові корені, починаючи з 137" src="https://cdn.freecodecamp.org/curriculum/project-euler/digital-root-clocks.gif" style="background-color: white; padding: 10px;" />

Сему і Максу було запропоновано перетворити два звичайних електронних годинники на два "кореневі" годинники.

Електронний кореневий годинник - це такий, що крок за кроком вираховує корені чисел.

Якщо годиннику дати число, воно спочатку відображається на циферблаті, а потім годинник починає розрахунки, показуючи усі проміжні значення, поки не покаже результат. Наприклад, якщо годиннику дати число 137, він покаже: `137` → `11` → `2`, після чого циферблат стане чорним, доки не введуть нове число.

Кожне число складається з певних світлових елементів: три горизонтальні (верх, середина, низ) і чотири вертикальні (зверху зліва, зверху справа, знизу зліва, знизу справа). Цифра `1` складається з вертикалі зверху справа і знизу справа, цифра `4` складається з горизонталі посередині і вертикалі зверху зліва, зверху справа і знизу справа. У цифрі `8` висвічуються усі елементи.

Годинники споживають енергію лише тоді, коли хоча б деякі елементи ввімкнені. Щоб показати `2` необхідно 5 переходів, а для `7` - лише 4.

Сем і Макс створили два різних годинники.

До годинника Сема ввели наприклад цифру 137: годинник показує `137`, потім циферблат вимикається, і з'являється наступна цифра (`11`), потім циферблат знову вимикається, і показує останню цифру (`2`), яка, через певний час зникає.

Наприклад, для номера 137, годиннику Сема необхідно:

- `137`: $(2 + 5 + 4) × 2 = 22$ переходів (`137` on/off).
- `11`: $(2 + 2) × 2 = 8$ переходів(`11` on/off).
- `2`: $(5) × 2 = 10$ переходів (`2` on/off).

Усього 40 переходів.

Годинник Макса працює по-іншому. Замість того, щоб вимикати всю панель, він досить розумний, щоб вимкнути лише ті сегменти, які не будуть потрібні для наступного числа.

Для числа 137, годинник Макса потребує:

- `137` : $2 + 5 + 4 = 11$ переходів (`137` on), $7$ переходів (для того, щоб виключити сегменти, які є непотрібними для числа `11`).
- `11` : $0$ переходів (число `11` уже ввімкнено правильно), $3$ переходи (виключити першу `1` і нижню частини другого `1`; верхня частина є спільна з числом `2`).
- `2` : $4$ переходи (увімкнути інші сегменти щоб отримати `2`), $5$ переходів(вимкнути число`2`).

Загалом 30 переходів.

Звичайно, годинник Макса витрачає менше енергії ніж годинник Сема. Два годинники використовують всі прості числа в межах $A = {10}^7$ and $B = 2 × {10}^7$. Знайдіть різницю між загальною кількістю переходів, яка є необхідною для годинника Сема та Макса.

# --hints--

`digitalRootClocks()` має повернути `13625242`.

```js
assert.strictEqual(digitalRootClocks(), 13625242);
```

# --seed--

## --seed-contents--

```js
function digitalRootClocks() {

  return true;
}

digitalRootClocks();
```

# --solutions--

```js
// solution required
```
