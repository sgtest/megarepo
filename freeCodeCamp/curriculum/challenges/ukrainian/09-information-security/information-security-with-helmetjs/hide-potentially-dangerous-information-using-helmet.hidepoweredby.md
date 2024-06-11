---
id: 587d8247367417b2b2512c37
title: Приховайте потенційно небезпечну інформацію за допомогою helmet.hidePoweredBy()
challengeType: 2
forumTopicId: 301580
dashedName: hide-potentially-dangerous-information-using-helmet-hidepoweredby
---

# --description--

Нагадуємо, що цей проєкт створюється на основі наступного стартового проєкту на <a href="https://gitpod.io/?autostart=true#https://github.com/freeCodeCamp/boilerplate-infosec/" target="_blank" rel="noopener noreferrer nofollow">Gitpod</a> або клонований з <a href="https://github.com/freeCodeCamp/boilerplate-infosec/" target="_blank" rel="noopener noreferrer nofollow">GitHub</a>.

Хакери можуть експлуатувати відомі вразливості у Express/Node, якщо побачать, що ваш сайт працює на Express. `X-Powered-By: Express` надсилається в кожному запиті від Express за замовчуванням. Використайте проміжне програмне забезпечення `helmet.hidePoweredBy()`, щоб вилучити заголовок X-Powered-By.

# --hints--

Проміжне ПЗ helmet.hidePoweredBy() повинне бути встановлене правильно

```js
(getUserInput) =>
  $.get(getUserInput('url') + '/_api/app-info').then(
    (data) => {
      assert.include(data.appStack, 'hidePoweredBy');
      assert.notEqual(data.headers['x-powered-by'], 'Express');
    },
    (xhr) => {
      throw new Error(xhr.responseText);
    }
  );
```

