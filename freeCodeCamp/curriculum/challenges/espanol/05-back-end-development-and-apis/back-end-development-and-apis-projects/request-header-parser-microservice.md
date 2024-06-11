---
id: bd7158d8c443edefaeb5bdff
title: Microservicio de analizador de solicitud de encabezado
challengeType: 4
forumTopicId: 301507
dashedName: request-header-parser-microservice
---

# --description--

Crea una aplicación full stack de JavaScript que sea funcionalmente similar a esta: <a href="https://request-header-parser-microservice.freecodecamp.rocks/" target="_blank" rel="noopener noreferrer nofollow">https://request-header-parser-microservice.freecodecamp.rocks/</a>. Trabajar en este proyecto implicará escribir tu código utilizando uno de los siguientes métodos:

-   Clone este repositorio de <a href="https://github.com/freeCodeCamp/boilerplate-project-headerparser/" target="_blank" rel="noopener noreferrer nofollow"> GitHub</a> y complete estos desafíos localmente.
-   Use <a href="https://gitpod.io/?autostart=true#https://github.com/freeCodeCamp/boilerplate-project-headerparser/" target="_blank" rel="noopener noreferrer nofollow">our Gitpod starter project</a> to complete your project.
-   Utiliza un constructor de sitios de tu elección para completar el proyecto. Asegúrate de incorporar todos los archivos de nuestro repositorio de GitHub.

# --hints--

Debes proporcionar tu propio proyecto, no la URL del ejemplo.

```js
(getUserInput) => {
  assert(
    !/.*\/request-header-parser-microservice\.freecodecamp\.rocks/.test(
      getUserInput('url')
    )
  );
};
```

Una petición a `/api/whoami` debe devolver un objeto JSON con tu dirección IP en la clave `ipaddress`.

```js
(getUserInput) =>
  $.get(getUserInput('url') + '/api/whoami').then(
    (data) => assert(data.ipaddress && data.ipaddress.length > 0),
    (xhr) => {
      throw new Error(xhr.responseText);
    }
  );
```

Una petición a `/api/whoami` debe devolver un objeto JSON con tu idioma preferido en la clave `language`.

```js
(getUserInput) =>
  $.get(getUserInput('url') + '/api/whoami').then(
    (data) => assert(data.language && data.language.length > 0),
    (xhr) => {
      throw new Error(xhr.responseText);
    }
  );
```

Una petición a `/api/whoami` debe devolver un objeto JSON con tu software en la clave de `software`.

```js
(getUserInput) =>
  $.get(getUserInput('url') + '/api/whoami').then(
    (data) => assert(data.software && data.software.length > 0),
    (xhr) => {
      throw new Error(xhr.responseText);
    }
  );
```

