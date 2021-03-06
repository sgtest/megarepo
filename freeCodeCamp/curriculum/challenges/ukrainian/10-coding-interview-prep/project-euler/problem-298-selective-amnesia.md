---
id: 5900f4971000cf542c50ffa9
title: 'Завдання 298: Вибіркова Амнезія'
challengeType: 1
forumTopicId: 301950
dashedName: problem-298-selective-amnesia
---

# --description--

Ларрі та Робін грають у гру на пам'ять, що включає послідовність випадкових чисел від 1 до 10 включно, які називаються по одному за раз. Кожен гравець може запам'ятати до 5 попередніх цифр. Коли назване число знаходиться в пам’яті гравця, то цьому гравцеві нараховується очко. Якщо це не так, гравець додає названий номер до своєї пам'яті, видаляючи інший номер, якщо його пам'ять заповнена.

Обидва гравці починають з порожньої пам'яті. Обидва гравці завжди додають у свою пам'ять нові пропущені номери, але використовують різні стратегії, вирішуючи, який номер видалити: стратегія Ларрі полягає у видаленні номера, який не називався протягом тривалого часу. Стратегія Робіна полягає в тому, щоб видалити число, яке було в пам’яті найдовше.

Приклад гри:

| Черга | Названий номер | Пам'ять Ларрі | Очки Ларрі | Пам'ять Робіна | Очки Робіна |
| ----- | -------------- | -------------:| ---------- | -------------- | ----------- |
| 1     | 1              |             1 | 0          | 1              | 0           |
| 2     | 2              |           1,2 | 0          | 1,2            | 0           |
| 3     | 4              |         1,2,4 | 0          | 1,2,4          | 0           |
| 4     | 6              |       1,2,4,6 | 0          | 1,2,4,6        | 0           |
| 5     | 1              |       1,2,4,6 | 1          | 1,2,4,6        | 1           |
| 6     | 8              |     1,2,4,6,8 | 1          | 1,2,4,6,8      | 1           |
| 7     | 10             |    1,4,6,8,10 | 1          | 2,4,6,8,10     | 1           |
| 8     | 2              |    1,2,6,8,10 | 1          | 2,4,6,8,10     | 2           |
| 9     | 4              |    1,2,4,8,10 | 1          | 2,4,6,8,10     | 3           |
| 10    | 1              |    1,2,4,8,10 | 2          | 1,4,6,8,10     | 3           |

Позначивши очки Ларрі $L$ і очки Робіна $R$, яким буде очікуване значення $|L - R|$ після 50 ходів? Дайте відповідь, округлену до восьми знаків після коми, у форматі x.xxxxxxxx .

# --hints--

`selectiveAmnesia()` має повернути `1.76882294`.

```js
assert.strictEqual(selectiveAmnesia(), 1.76882294);
```

# --seed--

## --seed-contents--

```js
function selectiveAmnesia() {

  return true;
}

selectiveAmnesia();
```

# --solutions--

```js
// solution required
```
