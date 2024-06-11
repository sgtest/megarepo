---
id: 587d8247367417b2b2512c39
title: >-
  Mitigar el Riesgo de Ataques de Secuencia de Comandos Cruzados de Sitio (XSS) con helmet.xssFilter()
challengeType: 2
forumTopicId: 301583
dashedName: mitigate-the-risk-of-cross-site-scripting-xss-attacks-with-helmet-xssfilter
---

# --description--

As a reminder, this project is being built upon the following starter project on <a href="https://gitpod.io/?autostart=true#https://github.com/freeCodeCamp/boilerplate-infosec/" target="_blank" rel="noopener noreferrer nofollow">Gitpod</a>, or cloned from <a href="https://github.com/freeCodeCamp/boilerplate-infosec/" target="_blank" rel="noopener noreferrer nofollow">GitHub</a>.

Secuencias de Comando de Sitio-Cruzado (XSS) es un ataque de tipo frecuente donde instrucciones maliciosas son insertadas dentro de páginas vulnerables, con el propósito de robar información sensible como información almacenada de sesión (cookies), o contraseñas.

The basic rule to lower the risk of an XSS attack is simple: "Never trust user's input". Como desarrollador siempre deberías limpiar toda la información que provenga desde el exterior. Esto incluye datos provenientes de formularios, consultas urls GET, e incluso de cuerpos POST. Sanear significa que tu deberías encontrar y los caracteres que puedan ser peligrosos por ej.: &lt;, >.

Navegadores actuales pueden ayudar a mitigar el riesgo mediante la adopción de mejores estrategias de software. A menudo esto es configurable via cabeceras http.

La cabecera X-XSS-Protection HTTP es una protección básica. El navegador detecta una pontencial sentencia maliciosa insertada usando un filtro heurístico. Si la cabecera esta habilitada, el navegador cambia el código de la sentencia maliciosa, neutralizándola. Todavía tiene un soporte limitado.

# --instructions--

Usa `helmet.xssFilter()` para sanear la entrada envíada a su servidor.

# --hints--

helmet.xssFilter() middleware debe ser montado correctamente

```js
(getUserInput) =>
  $.get(getUserInput('url') + '/_api/app-info').then(
    (data) => {
      assert.include(data.appStack, 'xXssProtection');
      assert.property(data.headers, 'x-xss-protection');
    },
    (xhr) => {
      throw new Error(xhr.responseText);
    }
  );
```

