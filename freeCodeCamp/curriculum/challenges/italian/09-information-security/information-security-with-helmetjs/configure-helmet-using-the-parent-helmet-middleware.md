---
id: 587d8249367417b2b2512c40
title: Configurare Helmet usando il middleware 'genitore' helmet()
challengeType: 2
forumTopicId: 301575
dashedName: configure-helmet-using-the-parent-helmet-middleware
---

# --description--

As a reminder, this project is being built upon the following starter project on <a href="https://gitpod.io/?autostart=true#https://github.com/freeCodeCamp/boilerplate-infosec/" target="_blank" rel="noopener noreferrer nofollow">Gitpod</a>, or cloned from <a href="https://github.com/freeCodeCamp/boilerplate-infosec/" target="_blank" rel="noopener noreferrer nofollow">GitHub</a>.

`app.use(helmet())` includerà automaticamente tutto il middleware introdotto sopra, tranne `noCache()`, e `contentSecurityPolicy()`, ma questi possono essere abilitati se necessario. È inoltre possibile disabilitare o configurare qualsiasi altro middleware singolarmente, utilizzando un oggetto di configurazione.

**Esempio:**

```js
app.use(helmet({
  frameguard: {         // configure
    action: 'deny'
  },
  contentSecurityPolicy: {    // enable and configure
    directives: {
      defaultSrc: ["'self'"],
      styleSrc: ['style.com'],
    }
  },
  dnsPrefetchControl: false     // disable
}))
```

Abbiamo introdotto ogni middleware separatamente per scopi didattici e per facilità di test. L'utilizzo del middleware 'genitore' `helmet()` è facile da implementare in un progetto reale.

# --hints--

nessun test - è una sfida descrittiva

```js
assert(true);
```

