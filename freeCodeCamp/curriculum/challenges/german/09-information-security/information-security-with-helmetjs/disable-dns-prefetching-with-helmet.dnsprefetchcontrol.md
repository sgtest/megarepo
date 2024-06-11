---
id: 587d8248367417b2b2512c3d
title: DNS Prefetching mit helmet.dnsPrefetchControl() deaktivieren
challengeType: 2
forumTopicId: 301577
dashedName: disable-dns-prefetching-with-helmet-dnsprefetchcontrol
---

# --description--

As a reminder, this project is being built upon the following starter project on <a href="https://gitpod.io/?autostart=true#https://github.com/freeCodeCamp/boilerplate-infosec/" target="_blank" rel="noopener noreferrer nofollow">Gitpod</a>, or cloned from <a href="https://github.com/freeCodeCamp/boilerplate-infosec/" target="_blank" rel="noopener noreferrer nofollow">GitHub</a>.

Um die Leistung zu verbessern, rufen die meisten Browser DNS-Einträge für die Links auf einer Seite vorab ab. Auf diese Weise ist die Ziel-IP bereits bekannt, wenn der Nutzer auf einen Link klickt. This may lead to over-use of the DNS service (if you own a big website, visited by millions people…), privacy issues (one eavesdropper could infer that you are on a certain page), or page statistics alteration (some links may appear visited even if they are not). Wenn du ein hohes Sicherheitsbedürfnis hast, kannst du das DNS-Prefetching deaktivieren, was allerdings mit Leistungseinbußen verbunden ist.

# --instructions--

Verwende die `helmet.dnsPrefetchControl()`-Methode auf deinem Server.

# --hints--

helmet.dnsPrefetchControl() middleware should be mounted correctly

```js
(getUserInput) =>
  $.get(getUserInput('url') + '/_api/app-info').then(
    (data) => {
      assert.include(data.appStack, 'dnsPrefetchControl');
      assert.equal(data.headers['x-dns-prefetch-control'], 'off');
    },
    (xhr) => {
      throw new Error(xhr.responseText);
    }
  );
```

