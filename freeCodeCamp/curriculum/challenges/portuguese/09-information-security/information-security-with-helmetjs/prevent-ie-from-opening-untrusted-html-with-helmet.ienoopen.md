---
id: 587d8248367417b2b2512c3b
title: Impedir que o IE abra HTML não confiável com helmet.ieNoOpen()
challengeType: 2
forumTopicId: 301584
dashedName: prevent-ie-from-opening-untrusted-html-with-helmet-ienoopen
---

# --description--

Lembre-se: este projeto está sendo criado conforme o seguinte projeto inicial em <a href="https://gitpod.io/?autostart=true#https://github.com/freeCodeCamp/boilerplate-infosec/" target="_blank" rel="noopener noreferrer nofollow">Gitpod</a>, ou clonado de <a href="https://github.com/freeCodeCamp/boilerplate-infosec/" target="_blank" rel="noopener noreferrer nofollow">GitHub</a>.

Alguns aplicativos da web servirão HTML não confiável para download. Algumas versões do Internet Explorer abrem esses arquivos HTML no contexto do seu site. Isto significa que uma página HTML não confiável pode começar a fazer coisas ruins no contexto de suas páginas. Este middleware define o cabeçalho X-Download-Options como noopen. Isto impedirá que usuários do IE executem downloads no contexto dos sites confiáveis.

# --instructions--

Use o método `helmet.ieNoOpen()` no seu servidor.

# --hints--

O middleware helmet.ieNoOpen() deve ser montado corretamente

```js
(getUserInput) =>
  $.get(getUserInput('url') + '/_api/app-info').then(
    (data) => {
      assert.include(data.appStack, 'ienoopen');
      assert.equal(data.headers['x-download-options'], 'noopen');
    },
    (xhr) => {
      throw new Error(xhr.responseText);
    }
  );
```

